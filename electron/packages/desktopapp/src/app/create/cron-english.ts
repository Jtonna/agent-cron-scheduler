/**
 * cron-english
 *
 * Tiny no-dep cron-to-English helper. Handles a handful of common 5-field
 * patterns and falls back to "Custom schedule" for anything else. Intended
 * purely for the schedule preview pill on the workflow editor — not a
 * full cron-parser substitute.
 *
 * Add new pattern entries here when one becomes common enough to deserve
 * a human label.
 */

const DAY_NAMES = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
];

function formatTime(hour: number, minute: number): string {
  if (hour === 0 && minute === 0) return "midnight";
  if (hour === 12 && minute === 0) return "noon";
  const period = hour >= 12 ? "PM" : "AM";
  const h12 = hour % 12 === 0 ? 12 : hour % 12;
  const mm = minute.toString().padStart(2, "0");
  return `${h12}:${mm} ${period}`;
}

/**
 * Returns a human-readable description of the given cron expression, or
 * "Custom schedule" when the pattern isn't recognised. Strips a 6th
 * (seconds) leading field if present so 6-field crons get the same
 * coverage as 5-field crons.
 */
export function cronToEnglish(cron: string): string {
  const trimmed = cron.trim();
  if (!trimmed) return "No schedule";

  const parts = trimmed.split(/\s+/);
  // Drop an optional leading seconds field so 6-field crons are coerced
  // to the 5-field shape this helper understands.
  const fields = parts.length === 6 ? parts.slice(1) : parts;
  if (fields.length !== 5) return "Custom schedule";

  const [min, hour, dom, mon, dow] = fields;

  // Every minute
  if (min === "*" && hour === "*" && dom === "*" && mon === "*" && dow === "*") {
    return "Every minute";
  }
  // Every N minutes
  const everyMinMatch = /^\*\/(\d+)$/.exec(min);
  if (everyMinMatch && hour === "*" && dom === "*" && mon === "*" && dow === "*") {
    return `Every ${everyMinMatch[1]} minutes`;
  }
  // Every hour (on minute 0 or :MM)
  if (hour === "*" && dom === "*" && mon === "*" && dow === "*") {
    const m = parseInt(min, 10);
    if (!Number.isNaN(m)) {
      return m === 0 ? "Every hour" : `Every hour at :${m.toString().padStart(2, "0")}`;
    }
  }
  // Every N hours
  const everyHourMatch = /^\*\/(\d+)$/.exec(hour);
  if (everyHourMatch && dom === "*" && mon === "*" && dow === "*") {
    return `Every ${everyHourMatch[1]} hours`;
  }
  // Daily at H:M
  if (dom === "*" && mon === "*" && dow === "*") {
    const m = parseInt(min, 10);
    const h = parseInt(hour, 10);
    if (!Number.isNaN(m) && !Number.isNaN(h)) {
      return `Every day at ${formatTime(h, m)}`;
    }
  }
  // Weekdays at H:M
  if (dom === "*" && mon === "*" && dow === "1-5") {
    const m = parseInt(min, 10);
    const h = parseInt(hour, 10);
    if (!Number.isNaN(m) && !Number.isNaN(h)) {
      return `Weekdays at ${formatTime(h, m)}`;
    }
  }
  // Specific weekday at H:M (single digit dow)
  if (dom === "*" && mon === "*" && /^[0-6]$/.test(dow)) {
    const m = parseInt(min, 10);
    const h = parseInt(hour, 10);
    if (!Number.isNaN(m) && !Number.isNaN(h)) {
      const day = DAY_NAMES[parseInt(dow, 10)];
      return `Every ${day} at ${formatTime(h, m)}`;
    }
  }
  // Monthly on the Nth at H:M
  if (mon === "*" && dow === "*" && /^\d+$/.test(dom)) {
    const m = parseInt(min, 10);
    const h = parseInt(hour, 10);
    const d = parseInt(dom, 10);
    if (!Number.isNaN(m) && !Number.isNaN(h)) {
      return `Monthly on the ${ordinal(d)} at ${formatTime(h, m)}`;
    }
  }

  return "Custom schedule";
}

function ordinal(n: number): string {
  const s = ["th", "st", "nd", "rd"];
  const v = n % 100;
  return n + (s[(v - 20) % 10] || s[v] || s[0]);
}
