#!/usr/bin/env python3
"""
External integration test runner for agent-cron-scheduler.

Talks to a running ACS daemon over HTTP. Loads a workflow JSON from disk,
creates it, triggers a manual run, polls until completion, asserts every
step succeeded, prints a summary, and (by default) deletes the workflow.

Python 3.8+ stdlib only. No external dependencies.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request
from typing import Any, Dict, List, Optional, Tuple


# ---------- tiny ANSI color helpers (TTY-gated) ----------

_USE_COLOR = sys.stdout.isatty() and os.environ.get("NO_COLOR") is None


def _c(code: str, s: str) -> str:
    if not _USE_COLOR:
        return s
    return f"\x1b[{code}m{s}\x1b[0m"


def dim(s: str) -> str:
    return _c("2", s)


def bold(s: str) -> str:
    return _c("1", s)


def red(s: str) -> str:
    return _c("31", s)


def green(s: str) -> str:
    return _c("32", s)


def yellow(s: str) -> str:
    return _c("33", s)


def cyan(s: str) -> str:
    return _c("36", s)


# ---------- HTTP helpers ----------


class ApiError(Exception):
    def __init__(self, status: int, body: str, url: str):
        super().__init__(f"HTTP {status} from {url}: {body}")
        self.status = status
        self.body = body
        self.url = url


def _request(
    method: str,
    url: str,
    body: Optional[Dict[str, Any]] = None,
    timeout: float = 30.0,
) -> Tuple[int, str]:
    data = None
    headers = {"Accept": "application/json"}
    if body is not None:
        data = json.dumps(body).encode("utf-8")
        headers["Content-Type"] = "application/json"

    req = urllib.request.Request(url, data=data, method=method, headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return resp.status, resp.read().decode("utf-8", errors="replace")
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode("utf-8", errors="replace")
    except urllib.error.URLError as e:
        raise ConnectionError(
            f"Could not reach daemon at {url}. Is the daemon running? ({e.reason})"
        ) from e


def api_json(
    method: str,
    base: str,
    path: str,
    body: Optional[Dict[str, Any]] = None,
    timeout: float = 30.0,
) -> Any:
    url = base.rstrip("/") + path
    status, text = _request(method, url, body=body, timeout=timeout)
    if status >= 400:
        raise ApiError(status, text, url)
    if not text:
        return None
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return text


def api_text(method: str, base: str, path: str, timeout: float = 30.0) -> str:
    url = base.rstrip("/") + path
    status, text = _request(method, url, timeout=timeout)
    if status >= 400:
        raise ApiError(status, text, url)
    return text


# ---------- assertions / formatting ----------


# kinds where exit_code is meaningful (process-based)
KINDS_WITH_EXIT_CODE = {"shell", "http"}


def fmt_duration(ms: Optional[int]) -> str:
    if ms is None:
        return "-"
    if ms < 1000:
        return f"{ms}ms"
    return f"{ms / 1000:.2f}s"


def print_summary(steps: List[Dict[str, Any]]) -> None:
    headers = ["step_id", "kind", "status", "exit_code", "duration"]
    rows = []
    for s in steps:
        rows.append(
            [
                str(s.get("id", "?")),
                str(s.get("kind", "?")),
                str(s.get("status", "?")),
                "-" if s.get("exit_code") is None else str(s.get("exit_code")),
                fmt_duration(s.get("duration_ms")),
            ]
        )

    widths = [max(len(h), *(len(r[i]) for r in rows)) for i, h in enumerate(headers)]

    def fmt_row(r: List[str]) -> str:
        return "  " + "  ".join(c.ljust(widths[i]) for i, c in enumerate(r))

    print()
    print(bold("  Step summary:"))
    print(dim(fmt_row(headers)))
    print(dim("  " + "  ".join("-" * w for w in widths)))
    for r in rows:
        status = r[2].lower()
        line = fmt_row(r)
        if status == "completed":
            print(green(line))
        elif status in ("failed", "errored", "cancelled"):
            print(red(line))
        else:
            print(yellow(line))
    print()


def extract_agent_result(log_text: str) -> Optional[str]:
    """Best-effort: scan the run log for an agent 'result' line and return it."""
    last_result: Optional[str] = None
    for raw in log_text.splitlines():
        line = raw.strip()
        if not line or not line.startswith("{"):
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            continue
        # Claude Code CLI streams JSON objects; final message often has type=result
        if isinstance(obj, dict):
            if obj.get("type") == "result" and isinstance(obj.get("result"), str):
                last_result = obj["result"]
            elif obj.get("subtype") == "success" and isinstance(obj.get("result"), str):
                last_result = obj["result"]
    return last_result


# ---------- main test ----------


def expected_compose_step_id(mood: str) -> str:
    if mood == "happy":
        return "compose_happy"
    if mood == "tired":
        return "compose_tired"
    return "compose_neutral"


def run_test(
    daemon_url: str,
    workflow_path: str,
    trigger_input: Optional[Dict[str, Any]],
    timeout_secs: int,
    keep: bool,
) -> int:
    print(bold("ACS integration test"))
    print(f"  daemon   : {cyan(daemon_url)}")
    print(f"  workflow : {cyan(workflow_path)}")
    print(f"  timeout  : {timeout_secs}s")
    print(f"  keep     : {keep}")
    print()

    # Load workflow JSON
    if not os.path.isfile(workflow_path):
        print(red(f"Workflow file not found: {workflow_path}"))
        return 2
    with open(workflow_path, "r", encoding="utf-8") as f:
        workflow_def = json.load(f)

    # Determine the mood we expect to route to
    if trigger_input is not None and isinstance(trigger_input.get("mood"), str):
        mood = trigger_input["mood"]
    else:
        mood = (workflow_def.get("default_input") or {}).get("mood", "")
    expected_compose = expected_compose_step_id(mood)

    workflow_id: Optional[str] = None
    exit_code = 0

    try:
        # 1. Create workflow
        print(bold("1. Creating workflow..."))
        try:
            created = api_json("POST", daemon_url, "/api/workflows", body=workflow_def)
        except ApiError as e:
            print(red(f"   create failed: HTTP {e.status}"))
            print(dim(f"   response body: {e.body}"))
            return 1
        workflow_id = created.get("id") if isinstance(created, dict) else None
        if not workflow_id:
            print(red("   create did not return an id"))
            print(dim(f"   response: {json.dumps(created)[:500]}"))
            return 1
        print(green(f"   created workflow id={workflow_id}"))

        # 2. Trigger
        print(bold("2. Triggering run..."))
        trigger_body: Dict[str, Any] = {}
        if trigger_input is not None:
            trigger_body = {"input": trigger_input}
        try:
            triggered = api_json(
                "POST",
                daemon_url,
                f"/api/workflows/{workflow_id}/trigger",
                body=trigger_body,
            )
        except ApiError as e:
            print(red(f"   trigger failed: HTTP {e.status}"))
            print(dim(f"   response body: {e.body}"))
            return 1
        run_id = (
            triggered.get("run_id") or triggered.get("id")
            if isinstance(triggered, dict)
            else None
        )
        if not run_id:
            print(red("   trigger did not return a run_id"))
            print(dim(f"   response: {json.dumps(triggered)[:500]}"))
            return 1
        print(green(f"   run_id={run_id}"))

        # 3. Poll until terminal
        print(bold("3. Polling run status (dot per poll, every 2s)..."))
        deadline = time.monotonic() + timeout_secs
        terminal = {"Completed", "Failed", "Errored", "Cancelled"}
        run: Optional[Dict[str, Any]] = None
        sys.stdout.write("   ")
        sys.stdout.flush()
        while True:
            try:
                run = api_json("GET", daemon_url, f"/api/runs/{run_id}")
            except ApiError as e:
                print()
                print(red(f"   poll failed: HTTP {e.status}"))
                print(dim(f"   body: {e.body}"))
                return 1
            status = (run or {}).get("status", "?") if isinstance(run, dict) else "?"
            if status in terminal:
                print(f" {status}")
                break
            sys.stdout.write(".")
            sys.stdout.flush()
            if time.monotonic() > deadline:
                print()
                print(red(f"   timed out after {timeout_secs}s (last status={status})"))
                return 1
            time.sleep(2)

        assert isinstance(run, dict)
        steps = run.get("steps") or []
        print_summary(steps)

        # 4. Assertions
        status = run.get("status")
        all_ok = True
        if status != "Completed":
            print(red(f"FAIL: run.status = {status} (expected Completed)"))
            all_ok = False

        present_ids = {s.get("id") for s in steps}
        required_ids = {"init", "fetch_weather", "build_context", "route_mood", expected_compose}
        missing = required_ids - present_ids
        if missing:
            print(red(f"FAIL: missing expected step ids: {sorted(missing)}"))
            all_ok = False

        for s in steps:
            sid = s.get("id")
            sstatus = s.get("status")
            kind = s.get("kind")
            ec = s.get("exit_code")
            if sstatus != "Completed":
                print(red(f"FAIL: step {sid!r} status={sstatus}"))
                err = s.get("error")
                if err:
                    print(dim(f"   error: {err}"))
                all_ok = False
                continue
            if kind in KINDS_WITH_EXIT_CODE:
                if ec is not None and ec != 0:
                    print(red(f"FAIL: step {sid!r} exit_code={ec}"))
                    all_ok = False

        # 5. Fetch log & extract agent result
        print(bold("4. Fetching run log..."))
        try:
            log_text = api_text("GET", daemon_url, f"/api/runs/{run_id}/log")
        except ApiError as e:
            print(yellow(f"   could not fetch log: HTTP {e.status}"))
            log_text = ""
        result_text = extract_agent_result(log_text) if log_text else None
        if result_text:
            print(bold("   Agent final result:"))
            for line in result_text.splitlines() or [result_text]:
                print("     " + line)
        else:
            print(dim("   (no agent 'result' line found in log)"))

        print()
        if all_ok:
            print(green(bold("PASS")) + f"  workflow={workflow_id} run={run_id}")
            exit_code = 0
        else:
            print(red(bold("FAIL")) + f"  workflow={workflow_id} run={run_id}")
            exit_code = 1

    except ConnectionError as e:
        print(red(str(e)))
        return 1
    except KeyboardInterrupt:
        print(yellow("\nInterrupted."))
        exit_code = 130
    finally:
        if workflow_id and not keep:
            print()
            print(bold("Cleanup: deleting workflow..."))
            try:
                api_json("DELETE", daemon_url, f"/api/workflows/{workflow_id}")
                print(green(f"   deleted workflow id={workflow_id}"))
            except ApiError as e:
                print(yellow(f"   delete failed: HTTP {e.status}: {e.body}"))
            except ConnectionError as e:
                print(yellow(f"   delete failed: {e}"))
        elif workflow_id and keep:
            print()
            print(yellow(f"--keep set: leaving workflow id={workflow_id} in place"))

    return exit_code


def parse_args(argv: List[str]) -> argparse.Namespace:
    p = argparse.ArgumentParser(
        prog="acs-integration-test",
        description="Run an ACS workflow end-to-end against a live daemon.",
    )
    p.add_argument("--daemon-url", default="http://127.0.0.1:8377")
    p.add_argument(
        "--workflow",
        default=os.path.join(
            os.path.dirname(os.path.abspath(__file__)),
            "workflows",
            "weather-greeter-demo.json",
        ),
    )
    p.add_argument("--keep", action="store_true")
    p.add_argument(
        "--input",
        default=None,
        help='JSON string for trigger input (overrides default_input). E.g. \'{"mood":"tired"}\'',
    )
    p.add_argument("--timeout-secs", type=int, default=180)
    return p.parse_args(argv)


def main(argv: List[str]) -> int:
    args = parse_args(argv)
    trigger_input: Optional[Dict[str, Any]] = None
    if args.input is not None:
        try:
            trigger_input = json.loads(args.input)
        except json.JSONDecodeError as e:
            print(red(f"--input is not valid JSON: {e}"))
            return 2
        if not isinstance(trigger_input, dict):
            print(red("--input must be a JSON object"))
            return 2
    return run_test(
        daemon_url=args.daemon_url,
        workflow_path=args.workflow,
        trigger_input=trigger_input,
        timeout_secs=args.timeout_secs,
        keep=args.keep,
    )


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
