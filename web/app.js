/* ===================================================================== */
/* Agent Cron Scheduler — Web UI                                         */
/* Vanilla JS, no frameworks, no build tools.                            */
/* ===================================================================== */

(function () {
  "use strict";

  // -------------------------------------------------------------------
  // Configuration
  // -------------------------------------------------------------------
  var API_BASE = window.location.origin;
  var HEALTH_POLL_INTERVAL = 5000;    // ms
  var JOBS_POLL_INTERVAL = 10000;     // ms — fallback if SSE is down
  var SSE_RECONNECT_DELAY = 3000;     // ms
  var TOAST_DURATION = 4000;          // ms
  var MAX_LOG_LINES = 2000;

  // -------------------------------------------------------------------
  // State
  // -------------------------------------------------------------------
  var state = {
    jobs: [],
    selectedJobId: null,
    selectedJobName: null,
    editingJobId: null,
    deleteJobId: null,
    deleteJobName: null,
    healthy: false,
    uptime: 0,
    eventSource: null,
    healthTimer: null,
    jobsTimer: null,
    logLineCount: 0,
    sseConnected: false,
    selectedRunId: null,
    runs: [],
    runsTotal: 0
  };

  // -------------------------------------------------------------------
  // DOM References
  // -------------------------------------------------------------------
  var dom = {};

  function cacheDom() {
    dom.healthDot = document.getElementById("health-dot");
    dom.healthText = document.getElementById("health-text");
    dom.uptimeDisplay = document.getElementById("uptime-display");
    dom.jobTableBody = document.getElementById("job-table-body");
    dom.btnAddJob = document.getElementById("btn-add-job");
    dom.modalOverlay = document.getElementById("modal-overlay");
    dom.modalTitle = document.getElementById("modal-title");
    dom.modalClose = document.getElementById("modal-close");
    dom.jobForm = document.getElementById("job-form");
    dom.formJobId = document.getElementById("form-job-id");
    dom.formName = document.getElementById("form-name");
    dom.formSchedule = document.getElementById("form-schedule");
    dom.formExecType = document.getElementById("form-exec-type");
    dom.formExecValue = document.getElementById("form-exec-value");
    dom.formExecValueLabel = document.getElementById("form-exec-value-label");
    dom.formTimezone = document.getElementById("form-timezone");
    dom.formWorkingDir = document.getElementById("form-working-dir");
    dom.formEnvVars = document.getElementById("form-env-vars");
    dom.btnCancelForm = document.getElementById("btn-cancel-form");
    dom.btnSubmitForm = document.getElementById("btn-submit-form");
    dom.deleteOverlay = document.getElementById("delete-overlay");
    dom.deleteJobName = document.getElementById("delete-job-name");
    dom.deleteModalClose = document.getElementById("delete-modal-close");
    dom.btnCancelDelete = document.getElementById("btn-cancel-delete");
    dom.btnConfirmDelete = document.getElementById("btn-confirm-delete");
    dom.sidebar = document.getElementById("job-sidebar");
    dom.sidebarBackdrop = document.getElementById("sidebar-backdrop");
    dom.sidebarJobName = document.getElementById("sidebar-job-name");
    dom.btnCloseSidebar = document.getElementById("btn-close-sidebar");
    dom.btnSidebarRun = document.getElementById("btn-sidebar-run");
    dom.btnSidebarEdit = document.getElementById("btn-sidebar-edit");
    dom.btnSidebarDelete = document.getElementById("btn-sidebar-delete");
    dom.logOutput = document.getElementById("log-output");
    dom.btnClearLog = document.getElementById("btn-clear-log");
    dom.toastContainer = document.getElementById("toast-container");
    dom.runsList = document.getElementById("runs-list");
    dom.btnRefreshRuns = document.getElementById("btn-refresh-runs");
  }

  // -------------------------------------------------------------------
  // Utilities
  // -------------------------------------------------------------------

  /**
   * Format a duration in seconds to a human-readable string.
   */
  function formatUptime(seconds) {
    if (seconds < 60) return seconds + "s";
    if (seconds < 3600) return Math.floor(seconds / 60) + "m " + (seconds % 60) + "s";
    var h = Math.floor(seconds / 3600);
    var m = Math.floor((seconds % 3600) / 60);
    return h + "h " + m + "m";
  }

  /**
   * Convert an ISO timestamp string to a relative time description.
   * Returns an object { text, title } where title is the full ISO string.
   */
  function relativeTime(isoString) {
    if (!isoString) return { text: "--", title: "" };
    var date = new Date(isoString);
    if (isNaN(date.getTime())) return { text: "--", title: "" };
    var now = Date.now();
    var diffMs = now - date.getTime();
    var absDiff = Math.abs(diffMs);
    var future = diffMs < 0;
    var seconds = Math.floor(absDiff / 1000);
    var minutes = Math.floor(seconds / 60);
    var hours = Math.floor(minutes / 60);
    var days = Math.floor(hours / 24);
    var text;
    if (seconds < 5) {
      text = "just now";
    } else if (seconds < 60) {
      text = seconds + "s" + (future ? " from now" : " ago");
    } else if (minutes < 60) {
      text = minutes + "m" + (future ? " from now" : " ago");
    } else if (hours < 24) {
      text = hours + "h " + (minutes % 60) + "m" + (future ? " from now" : " ago");
    } else if (days < 7) {
      text = days + "d " + (hours % 24) + "h" + (future ? " from now" : " ago");
    } else {
      text = date.toLocaleDateString();
    }
    return { text: text, title: date.toLocaleString() };
  }

  /**
   * Parse env vars textarea "KEY=VALUE\n..." into a HashMap-like object.
   * Returns null if empty.
   */
  function parseEnvVars(text) {
    if (!text || !text.trim()) return null;
    var lines = text.trim().split("\n");
    var result = {};
    var hasEntries = false;
    for (var i = 0; i < lines.length; i++) {
      var line = lines[i].trim();
      if (!line) continue;
      var eqIdx = line.indexOf("=");
      if (eqIdx > 0) {
        result[line.substring(0, eqIdx).trim()] = line.substring(eqIdx + 1);
        hasEntries = true;
      }
    }
    return hasEntries ? result : null;
  }

  /**
   * Serialize env vars object back to "KEY=VALUE\n..." text.
   */
  function serializeEnvVars(envMap) {
    if (!envMap) return "";
    var lines = [];
    var keys = Object.keys(envMap);
    for (var i = 0; i < keys.length; i++) {
      lines.push(keys[i] + "=" + envMap[keys[i]]);
    }
    return lines.join("\n");
  }

  /**
   * Escape HTML to prevent XSS when inserting user data into the DOM.
   */
  function escapeHtml(str) {
    var div = document.createElement("div");
    div.appendChild(document.createTextNode(str));
    return div.innerHTML;
  }

  /**
   * Format a short timestamp for log lines (HH:MM:SS).
   */
  function logTimestamp(isoString) {
    if (!isoString) return "";
    var d = new Date(isoString);
    if (isNaN(d.getTime())) return "";
    var hh = String(d.getHours()).padStart(2, "0");
    var mm = String(d.getMinutes()).padStart(2, "0");
    var ss = String(d.getSeconds()).padStart(2, "0");
    return hh + ":" + mm + ":" + ss;
  }

  /**
   * Convert a 5-field cron expression to human-readable text.
   * Returns null for expressions too complex to describe simply.
   */
  function cronToHuman(expr) {
    if (!expr || typeof expr !== "string") return null;
    var parts = expr.trim().split(/\s+/);
    if (parts.length !== 5) return null;
    var min = parts[0], hour = parts[1], dom = parts[2], month = parts[3], dow = parts[4];

    function formatTime(h, m) {
      var hi = parseInt(h, 10), mi = parseInt(m, 10);
      var suffix = hi >= 12 ? "PM" : "AM";
      var h12 = hi % 12;
      if (h12 === 0) h12 = 12;
      if (mi === 0) return h12 + ":00 " + suffix;
      return h12 + ":" + String(mi).padStart(2, "0") + " " + suffix;
    }

    var dowNames = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

    function parseDow(field) {
      if (/^\d-\d$/.test(field)) {
        var s = parseInt(field[0], 10), e = parseInt(field[2], 10);
        if (s >= 0 && s <= 7 && e >= 0 && e <= 7) {
          return dowNames[s % 7] + "\u2013" + dowNames[e % 7];
        }
      }
      if (/^\d(,\d)+$/.test(field)) {
        return field.split(",").map(function (d) {
          var n = parseInt(d, 10);
          return (n >= 0 && n <= 7) ? dowNames[n % 7] : d;
        }).join(", ");
      }
      return null;
    }

    function ordinal(n) {
      var s = ["th", "st", "nd", "rd"];
      var v = n % 100;
      return n + (s[(v - 20) % 10] || s[v] || s[0]);
    }

    // Every minute
    if (min === "*" && hour === "*" && dom === "*" && month === "*" && dow === "*") return "Every minute";

    // Every N minutes
    var stepMin = min.match(/^\*\/(\d+)$/);
    if (stepMin && hour === "*" && dom === "*" && month === "*" && dow === "*") {
      var n = parseInt(stepMin[1], 10);
      return n === 1 ? "Every minute" : "Every " + n + " minutes";
    }

    // Every hour
    if (min === "0" && hour === "*" && dom === "*" && month === "*" && dow === "*") return "Every hour";

    // Every N hours
    var stepHour = hour.match(/^\*\/(\d+)$/);
    if (min === "0" && stepHour && dom === "*" && month === "*" && dow === "*") {
      var nh = parseInt(stepHour[1], 10);
      return nh === 1 ? "Every hour" : "Every " + nh + " hours";
    }

    // Hourly at :MM
    if (/^\d+$/.test(min) && hour === "*" && dom === "*" && month === "*" && dow === "*") {
      return "Hourly at :" + String(parseInt(min, 10)).padStart(2, "0");
    }

    // Daily at H:MM
    if (/^\d+$/.test(min) && /^\d+$/.test(hour) && dom === "*" && month === "*" && dow === "*") {
      if (parseInt(hour, 10) === 0 && parseInt(min, 10) === 0) return "Daily at midnight";
      if (parseInt(hour, 10) === 12 && parseInt(min, 10) === 0) return "Daily at noon";
      return "Daily at " + formatTime(hour, min);
    }

    // Weekly: specific day(s) of week
    if (/^\d+$/.test(min) && /^\d+$/.test(hour) && dom === "*" && month === "*" && dow !== "*") {
      var dowText = parseDow(dow);
      if (dowText) return "At " + formatTime(hour, min) + ", " + dowText;
    }

    // Monthly on the Nth
    if (/^\d+$/.test(min) && /^\d+$/.test(hour) && /^\d+$/.test(dom) && month === "*" && dow === "*") {
      var dayNum = parseInt(dom, 10);
      if (parseInt(hour, 10) === 0 && parseInt(min, 10) === 0) return "Monthly on the " + ordinal(dayNum);
      return "Monthly on the " + ordinal(dayNum) + " at " + formatTime(hour, min);
    }

    return null;
  }

  // -------------------------------------------------------------------
  // Toast Notifications
  // -------------------------------------------------------------------

  function showToast(message, type) {
    type = type || "info";
    var toast = document.createElement("div");
    toast.className = "toast toast-" + type;
    toast.textContent = message;
    dom.toastContainer.appendChild(toast);

    setTimeout(function () {
      toast.classList.add("toast-exit");
      setTimeout(function () {
        if (toast.parentNode) toast.parentNode.removeChild(toast);
      }, 300);
    }, TOAST_DURATION);
  }

  // -------------------------------------------------------------------
  // API Calls
  // -------------------------------------------------------------------

  function apiFetch(path, options) {
    options = options || {};
    var url = API_BASE + path;
    var fetchOptions = {
      method: options.method || "GET",
      headers: {}
    };
    if (options.body) {
      fetchOptions.headers["Content-Type"] = "application/json";
      fetchOptions.body = JSON.stringify(options.body);
    }
    return fetch(url, fetchOptions).then(function (response) {
      // For 204 No Content, return null
      if (response.status === 204) return { ok: true, status: 204, data: null };
      return response.json().then(function (data) {
        return { ok: response.ok, status: response.status, data: data };
      });
    }).catch(function (err) {
      return { ok: false, status: 0, data: null, error: err.message };
    });
  }

  function fetchHealth() {
    apiFetch("/health").then(function (res) {
      if (res.ok && res.data) {
        state.healthy = true;
        state.uptime = res.data.uptime_seconds || 0;
        updateHealthUI(true, res.data);
      } else {
        state.healthy = false;
        updateHealthUI(false, null);
      }
    });
  }

  function fetchJobs() {
    apiFetch("/api/jobs").then(function (res) {
      if (res.ok && Array.isArray(res.data)) {
        state.jobs = res.data;
        renderJobTable();
      } else if (!res.ok && res.status === 0) {
        // Connection failed — daemon likely not running
        state.healthy = false;
        updateHealthUI(false, null);
      }
    });
  }

  function createJob(jobData) {
    return apiFetch("/api/jobs", { method: "POST", body: jobData });
  }

  function updateJob(jobId, updateData) {
    return apiFetch("/api/jobs/" + jobId, { method: "PATCH", body: updateData });
  }

  function deleteJob(jobId) {
    return apiFetch("/api/jobs/" + jobId, { method: "DELETE" });
  }

  function enableJob(jobId) {
    return apiFetch("/api/jobs/" + jobId + "/enable", { method: "POST" });
  }

  function disableJob(jobId) {
    return apiFetch("/api/jobs/" + jobId + "/disable", { method: "POST" });
  }

  function triggerJob(jobId) {
    return apiFetch("/api/jobs/" + jobId + "/trigger", { method: "POST" });
  }

  function fetchRuns(jobId) {
    return apiFetch("/api/jobs/" + jobId + "/runs?limit=20&offset=0");
  }

  function fetchLog(runId) {
    return apiFetch("/api/runs/" + runId + "/log");
  }

  // -------------------------------------------------------------------
  // Health UI
  // -------------------------------------------------------------------

  function updateHealthUI(connected, healthData) {
    dom.healthDot.className = "health-dot " + (connected ? "connected" : "disconnected");
    if (connected && healthData) {
      dom.healthText.textContent =
        healthData.active_jobs + "/" + healthData.total_jobs + " active";
      dom.uptimeDisplay.textContent = "up " + formatUptime(healthData.uptime_seconds);
    } else {
      dom.healthText.textContent = "Disconnected";
      dom.uptimeDisplay.textContent = "";
    }
  }

  // -------------------------------------------------------------------
  // Job Table Rendering
  // -------------------------------------------------------------------

  function renderJobTable() {
    var tbody = dom.jobTableBody;
    if (state.jobs.length === 0) {
      tbody.innerHTML =
        '<tr class="empty-row"><td colspan="7" class="empty-message">No jobs configured. Click "+ Add Job" to create one.</td></tr>';
      return;
    }

    var html = "";
    for (var i = 0; i < state.jobs.length; i++) {
      var job = state.jobs[i];
      html += renderJobRow(job);
    }
    tbody.innerHTML = html;

    // Attach event listeners to newly rendered elements
    attachRowListeners();
    lucide.createIcons();
  }

  function renderJobRow(job) {
    var lastRun = relativeTime(job.last_run_at);
    var nextRun = relativeTime(job.next_run_at);

    // Exit code display
    var exitCodeHtml;
    if (job.last_exit_code === null || job.last_exit_code === undefined) {
      exitCodeHtml = '<span class="exit-code code-na">--</span>';
    } else if (job.last_exit_code === 0) {
      exitCodeHtml = '<span class="exit-code code-0">0</span>';
    } else {
      exitCodeHtml = '<span class="exit-code code-error">' + escapeHtml(String(job.last_exit_code)) + '</span>';
    }

    // Checked attribute for toggle
    var checkedAttr = job.enabled ? " checked" : "";

    return (
      '<tr class="job-row" data-job-id="' + escapeHtml(job.id) + '" data-job-name="' + escapeHtml(job.name) + '">' +
      '<td><span class="job-name">' + escapeHtml(job.name) + '</span></td>' +
      (function () { var h = cronToHuman(job.schedule); return h ? '<td><span class="schedule-human">' + escapeHtml(h) + '</span><span class="schedule-cron">' + escapeHtml(job.schedule) + '</span></td>' : '<td><span class="job-schedule">' + escapeHtml(job.schedule) + '</span></td>'; })() +
      '<td>' +
        '<label class="toggle-switch">' +
          '<input type="checkbox" class="toggle-enabled" data-job-id="' + escapeHtml(job.id) + '"' + checkedAttr + '>' +
          '<span class="toggle-slider"></span>' +
        '</label>' +
      '</td>' +
      '<td><span class="time-relative"' + (lastRun.title ? ' title="' + escapeHtml(lastRun.title) + '"' : '') + '>' + escapeHtml(lastRun.text) + '</span></td>' +
      '<td><span class="time-relative"' + (nextRun.title ? ' title="' + escapeHtml(nextRun.title) + '"' : '') + '>' + escapeHtml(nextRun.text) + '</span></td>' +
      '<td>' + exitCodeHtml + '</td>' +
      '<td>' +
        '<div class="actions-cell">' +
          '<button class="btn-icon trigger" title="Trigger" data-action="trigger" data-job-id="' + escapeHtml(job.id) + '"><i data-lucide="play"></i></button>' +
          '<button class="btn-icon logs" title="View Logs" data-action="logs" data-job-id="' + escapeHtml(job.id) + '" data-job-name="' + escapeHtml(job.name) + '"><i data-lucide="file-text"></i></button>' +
          '<button class="btn-icon edit" title="Edit" data-action="edit" data-job-id="' + escapeHtml(job.id) + '"><i data-lucide="pencil"></i></button>' +
          '<button class="btn-icon delete" title="Delete" data-action="delete" data-job-id="' + escapeHtml(job.id) + '" data-job-name="' + escapeHtml(job.name) + '"><i data-lucide="trash-2"></i></button>' +
        '</div>' +
      '</td>' +
      '</tr>'
    );
  }

  function attachRowListeners() {
    // Toggle enabled/disabled
    var toggles = document.querySelectorAll(".toggle-enabled");
    for (var i = 0; i < toggles.length; i++) {
      toggles[i].addEventListener("change", handleToggleEnabled);
    }

    // Action buttons
    var buttons = document.querySelectorAll(".btn-icon[data-action]");
    for (var j = 0; j < buttons.length; j++) {
      buttons[j].addEventListener("click", handleActionButton);
    }

    // Row click to open sidebar
    var rows = document.querySelectorAll(".job-row");
    for (var k = 0; k < rows.length; k++) {
      rows[k].addEventListener("click", handleRowClick);
    }
  }

  // -------------------------------------------------------------------
  // Event Handlers — Toggle
  // -------------------------------------------------------------------

  function handleToggleEnabled(e) {
    var jobId = e.target.getAttribute("data-job-id");
    var shouldEnable = e.target.checked;
    var toggle = e.target;

    var action = shouldEnable ? enableJob : disableJob;
    action(jobId).then(function (res) {
      if (res.ok) {
        showToast("Job " + (shouldEnable ? "enabled" : "disabled"), "success");
        fetchJobs();
      } else {
        // Revert the toggle
        toggle.checked = !shouldEnable;
        var msg = (res.data && res.data.message) ? res.data.message : "Failed to update job";
        showToast(msg, "error");
      }
    });
  }

  // -------------------------------------------------------------------
  // Event Handlers — Row Click
  // -------------------------------------------------------------------

  function handleRowClick(e) {
    // Don't open sidebar if user clicked an interactive element
    var target = e.target;
    while (target && target !== e.currentTarget) {
      if (target.tagName === "BUTTON" || target.tagName === "INPUT" ||
          target.tagName === "LABEL" || target.classList.contains("btn-icon") ||
          target.classList.contains("toggle-switch")) {
        return;
      }
      target = target.parentElement;
    }

    var jobId = e.currentTarget.getAttribute("data-job-id");
    var jobName = e.currentTarget.getAttribute("data-job-name");
    if (jobId) {
      openSidebar(jobId, jobName);
    }
  }

  // -------------------------------------------------------------------
  // Event Handlers — Action Buttons
  // -------------------------------------------------------------------

  function handleActionButton(e) {
    var btn = e.currentTarget;
    var action = btn.getAttribute("data-action");
    var jobId = btn.getAttribute("data-job-id");
    var jobName = btn.getAttribute("data-job-name");

    switch (action) {
      case "trigger":
        handleTrigger(jobId);
        break;
      case "logs":
        handleViewLogs(jobId, jobName);
        break;
      case "edit":
        handleEdit(jobId);
        break;
      case "delete":
        handleDeletePrompt(jobId, jobName);
        break;
    }
  }

  function handleTrigger(jobId) {
    triggerJob(jobId).then(function (res) {
      if (res.ok) {
        showToast("Job triggered", "success");
      } else {
        var msg = (res.data && res.data.message) ? res.data.message : "Failed to trigger job";
        showToast(msg, "error");
      }
    });
  }

  function handleViewLogs(jobId, jobName) {
    openSidebar(jobId, jobName);
  }

  function openSidebar(jobId, jobName) {
    state.selectedJobId = jobId;
    state.selectedJobName = jobName;
    state.selectedRunId = null;
    state.runs = [];
    state.runsTotal = 0;
    dom.sidebarJobName.textContent = jobName || jobId;
    dom.logOutput.innerHTML = "";
    dom.runsList.innerHTML = '<div class="runs-empty">Loading runs...</div>';
    state.logLineCount = 0;
    dom.sidebarBackdrop.classList.add("visible");
    dom.sidebar.classList.add("open");
    document.body.style.overflow = "hidden";
    loadRunsList(jobId);
    connectSSE();
  }

  function handleEdit(jobId) {
    // Find the job in current state
    var job = null;
    for (var i = 0; i < state.jobs.length; i++) {
      if (state.jobs[i].id === jobId) {
        job = state.jobs[i];
        break;
      }
    }
    if (!job) {
      showToast("Job not found", "error");
      return;
    }

    state.editingJobId = jobId;
    dom.modalTitle.textContent = "Edit Job";
    dom.btnSubmitForm.textContent = "Save Changes";
    dom.formJobId.value = jobId;
    dom.formName.value = job.name;
    dom.formSchedule.value = job.schedule;

    // Execution type
    if (job.execution && job.execution.type === "ScriptFile") {
      dom.formExecType.value = "ScriptFile";
      dom.formExecValueLabel.textContent = "Script File";
    } else {
      dom.formExecType.value = "ShellCommand";
      dom.formExecValueLabel.textContent = "Command";
    }
    dom.formExecValue.value = (job.execution && job.execution.value) ? job.execution.value : "";

    dom.formTimezone.value = job.timezone || "";
    dom.formWorkingDir.value = job.working_dir || "";
    dom.formEnvVars.value = serializeEnvVars(job.env_vars);

    openModal();
  }

  function handleDeletePrompt(jobId, jobName) {
    state.deleteJobId = jobId;
    state.deleteJobName = jobName;
    dom.deleteJobName.textContent = jobName || jobId;
    dom.deleteOverlay.classList.add("visible");
  }

  function handleConfirmDelete() {
    if (!state.deleteJobId) return;
    var jobId = state.deleteJobId;

    deleteJob(jobId).then(function (res) {
      if (res.ok || res.status === 204) {
        showToast("Job deleted", "success");
        closeDeleteModal();
        fetchJobs();
        // Close log viewer if viewing this job
        if (state.selectedJobId === jobId) {
          closeLogViewer();
        }
      } else {
        var msg = (res.data && res.data.message) ? res.data.message : "Failed to delete job";
        showToast(msg, "error");
      }
    });
  }

  // -------------------------------------------------------------------
  // Modal — Add / Edit Job
  // -------------------------------------------------------------------

  function openModal() {
    dom.modalOverlay.classList.add("visible");
    dom.formName.focus();
  }

  function closeModal() {
    dom.modalOverlay.classList.remove("visible");
    state.editingJobId = null;
    dom.jobForm.reset();
    dom.formJobId.value = "";
  }

  function openAddModal() {
    state.editingJobId = null;
    dom.modalTitle.textContent = "Add Job";
    dom.btnSubmitForm.textContent = "Create Job";
    dom.formJobId.value = "";
    dom.jobForm.reset();
    dom.formExecType.value = "ShellCommand";
    dom.formExecValueLabel.textContent = "Command";
    openModal();
  }

  function handleFormSubmit(e) {
    e.preventDefault();

    var execType = dom.formExecType.value;
    var execValue = dom.formExecValue.value.trim();
    var name = dom.formName.value.trim();
    var schedule = dom.formSchedule.value.trim();

    if (!name || !schedule || !execValue) {
      showToast("Please fill in all required fields", "error");
      return;
    }

    var envVars = parseEnvVars(dom.formEnvVars.value);
    var timezone = dom.formTimezone.value.trim() || undefined;
    var workingDir = dom.formWorkingDir.value.trim() || undefined;

    if (state.editingJobId) {
      // Update existing job
      var updatePayload = {
        name: name,
        schedule: schedule,
        execution: { type: execType, value: execValue }
      };
      if (timezone) updatePayload.timezone = timezone;
      if (workingDir) updatePayload.working_dir = workingDir;
      if (envVars) updatePayload.env_vars = envVars;

      dom.btnSubmitForm.disabled = true;
      updateJob(state.editingJobId, updatePayload).then(function (res) {
        dom.btnSubmitForm.disabled = false;
        if (res.ok) {
          showToast("Job updated", "success");
          closeModal();
          fetchJobs();
        } else {
          var msg = (res.data && res.data.message) ? res.data.message : "Failed to update job";
          showToast(msg, "error");
        }
      });
    } else {
      // Create new job
      var createPayload = {
        name: name,
        schedule: schedule,
        execution: { type: execType, value: execValue }
      };
      if (timezone) createPayload.timezone = timezone;
      if (workingDir) createPayload.working_dir = workingDir;
      if (envVars) createPayload.env_vars = envVars;

      dom.btnSubmitForm.disabled = true;
      createJob(createPayload).then(function (res) {
        dom.btnSubmitForm.disabled = false;
        if (res.ok) {
          showToast("Job created", "success");
          closeModal();
          fetchJobs();
        } else {
          var msg = (res.data && res.data.message) ? res.data.message : "Failed to create job";
          showToast(msg, "error");
        }
      });
    }
  }

  // -------------------------------------------------------------------
  // Delete Modal
  // -------------------------------------------------------------------

  function closeDeleteModal() {
    dom.deleteOverlay.classList.remove("visible");
    state.deleteJobId = null;
    state.deleteJobName = null;
  }

  // -------------------------------------------------------------------
  // Log Viewer
  // -------------------------------------------------------------------

  function appendLogLine(text, cssClass) {
    if (state.logLineCount >= MAX_LOG_LINES) {
      // Remove oldest lines to prevent memory issues
      var firstChild = dom.logOutput.firstChild;
      if (firstChild) dom.logOutput.removeChild(firstChild);
      state.logLineCount--;
    }

    var span = document.createElement("span");
    span.className = "log-line" + (cssClass ? " " + cssClass : "");
    span.textContent = text;
    dom.logOutput.appendChild(span);
    dom.logOutput.appendChild(document.createTextNode("\n"));
    state.logLineCount++;

    // Auto-scroll to bottom
    dom.logOutput.scrollTop = dom.logOutput.scrollHeight;
  }

  function appendLogLineWithTimestamp(text, timestamp, cssClass) {
    if (state.logLineCount >= MAX_LOG_LINES) {
      var firstChild = dom.logOutput.firstChild;
      if (firstChild) dom.logOutput.removeChild(firstChild);
      // Also remove the newline text node
      firstChild = dom.logOutput.firstChild;
      if (firstChild && firstChild.nodeType === 3) dom.logOutput.removeChild(firstChild);
      state.logLineCount--;
    }

    var span = document.createElement("span");
    span.className = "log-line" + (cssClass ? " " + cssClass : "");

    var tsSpan = document.createElement("span");
    tsSpan.className = "log-timestamp";
    tsSpan.textContent = "[" + logTimestamp(timestamp) + "]";
    span.appendChild(tsSpan);
    span.appendChild(document.createTextNode(text));

    dom.logOutput.appendChild(span);
    dom.logOutput.appendChild(document.createTextNode("\n"));
    state.logLineCount++;

    dom.logOutput.scrollTop = dom.logOutput.scrollHeight;
  }

  function clearLog() {
    dom.logOutput.innerHTML = "";
    state.logLineCount = 0;
  }

  function closeSidebar() {
    dom.sidebar.classList.remove("open");
    dom.sidebarBackdrop.classList.remove("visible");
    document.body.style.overflow = "";
    state.selectedJobId = null;
    state.selectedJobName = null;
    state.selectedRunId = null;
    state.runs = [];
    state.runsTotal = 0;
    clearLog();
  }

  function closeLogViewer() {
    closeSidebar();
  }

  function loadRunsList(jobId) {
    fetchRuns(jobId).then(function (res) {
      if (res.ok && res.data && res.data.runs) {
        state.runs = res.data.runs;
        state.runsTotal = res.data.total || res.data.runs.length;
      } else {
        state.runs = [];
        state.runsTotal = 0;
      }
      renderRunsList();
      // Auto-select the newest run if nothing is selected
      if (!state.selectedRunId && state.runs.length > 0) {
        handleSelectRun(state.runs[0].run_id);
      } else if (state.runs.length === 0) {
        dom.runsList.innerHTML = '<div class="runs-empty">No runs yet</div>';
        dom.logOutput.innerHTML = "";
        appendLogLine("No previous runs. Listening for live output...", "log-info");
      }
    });
  }

  function renderRunsList() {
    if (state.runs.length === 0) {
      dom.runsList.innerHTML = '<div class="runs-empty">No runs yet</div>';
      return;
    }

    // Clear and rebuild with DOM methods (XSS-safe)
    dom.runsList.innerHTML = "";
    for (var i = 0; i < state.runs.length; i++) {
      var run = state.runs[i];
      var item = document.createElement("div");
      item.className = "run-item" + (run.run_id === state.selectedRunId ? " active" : "");
      item.setAttribute("data-run-id", run.run_id);

      var topRow = document.createElement("div");
      topRow.className = "run-item-top";

      var dot = document.createElement("span");
      var statusClass = "status-" + (run.status || "unknown").toLowerCase();
      dot.className = "run-status-dot " + statusClass;
      topRow.appendChild(dot);

      var label = document.createElement("span");
      label.className = "run-status-label";
      label.textContent = run.status || "Unknown";
      topRow.appendChild(label);

      if (run.exit_code !== null && run.exit_code !== undefined) {
        var exitSpan = document.createElement("span");
        exitSpan.className = "run-exit-code";
        exitSpan.textContent = "exit " + run.exit_code;
        topRow.appendChild(exitSpan);
      }

      item.appendChild(topRow);

      var timeEl = document.createElement("div");
      timeEl.className = "run-item-time";
      var ts = run.started_at || run.created_at;
      if (ts) {
        var d = new Date(ts);
        timeEl.textContent = isNaN(d.getTime()) ? "" : d.toLocaleString();
      }
      item.appendChild(timeEl);

      // Click handler via closure
      (function (runId) {
        item.addEventListener("click", function () {
          handleSelectRun(runId);
        });
      })(run.run_id);

      dom.runsList.appendChild(item);
    }
  }

  function handleSelectRun(runId) {
    state.selectedRunId = runId;

    // Update active class on run items
    var items = dom.runsList.querySelectorAll(".run-item");
    for (var i = 0; i < items.length; i++) {
      if (items[i].getAttribute("data-run-id") === runId) {
        items[i].classList.add("active");
      } else {
        items[i].classList.remove("active");
      }
    }

    // Clear log and load the selected run's log
    clearLog();

    // Find the run in state
    var run = null;
    for (var j = 0; j < state.runs.length; j++) {
      if (state.runs[j].run_id === runId) {
        run = state.runs[j];
        break;
      }
    }

    if (run && run.status === "Running") {
      appendLogLine("Streaming live output...", "log-info");
    }

    // Fetch the log content
    var logUrl = API_BASE + "/api/runs/" + runId + "/log";
    fetch(logUrl)
      .then(function (r) { return r.text(); })
      .then(function (text) {
        if (text) {
          var lines = text.split("\n");
          for (var k = 0; k < lines.length; k++) {
            if (lines[k]) appendLogLine(lines[k], "");
          }
        } else if (!run || run.status !== "Running") {
          appendLogLine("No log output for this run.", "log-info");
        }
      })
      .catch(function () {
        appendLogLine("Could not fetch log.", "log-failed");
      });
  }

  // -------------------------------------------------------------------
  // SSE (Server-Sent Events)
  // -------------------------------------------------------------------

  function connectSSE() {
    // Close existing connection
    if (state.eventSource) {
      state.eventSource.close();
      state.eventSource = null;
    }

    var url = API_BASE + "/api/events";
    try {
      state.eventSource = new EventSource(url);
    } catch (err) {
      state.sseConnected = false;
      scheduleSSEReconnect();
      return;
    }

    state.eventSource.onopen = function () {
      state.sseConnected = true;
    };

    state.eventSource.onerror = function () {
      state.sseConnected = false;
      if (state.eventSource) {
        state.eventSource.close();
        state.eventSource = null;
      }
      scheduleSSEReconnect();
    };

    // Listen for specific event types from the server
    state.eventSource.addEventListener("started", function (e) {
      handleSSEEvent("started", e.data);
    });

    state.eventSource.addEventListener("output", function (e) {
      handleSSEEvent("output", e.data);
    });

    state.eventSource.addEventListener("completed", function (e) {
      handleSSEEvent("completed", e.data);
    });

    state.eventSource.addEventListener("failed", function (e) {
      handleSSEEvent("failed", e.data);
    });

    state.eventSource.addEventListener("job_changed", function (e) {
      handleSSEEvent("job_changed", e.data);
    });
  }

  function scheduleSSEReconnect() {
    setTimeout(function () {
      if (!state.eventSource && state.healthy) {
        connectSSE();
      }
    }, SSE_RECONNECT_DELAY);
  }

  function handleSSEEvent(eventType, rawData) {
    var data;
    try {
      data = JSON.parse(rawData);
    } catch (err) {
      return;
    }

    // The server wraps event data with serde tagged enum:
    // { "event": "Started", "data": { ... } }
    var payload = data.data || data;

    switch (eventType) {
      case "started":
        handleJobStarted(payload);
        break;
      case "output":
        handleJobOutput(payload);
        break;
      case "completed":
        handleJobCompleted(payload);
        break;
      case "failed":
        handleJobFailed(payload);
        break;
      case "job_changed":
        handleJobChanged(payload);
        break;
    }
  }

  function handleJobStarted(data) {
    if (state.selectedJobId && data.job_id === state.selectedJobId) {
      // Create a synthetic run object and prepend to the runs list
      var newRun = {
        run_id: data.run_id,
        job_id: data.job_id,
        status: "Running",
        exit_code: null,
        started_at: data.timestamp,
        finished_at: null
      };
      state.runs.unshift(newRun);
      renderRunsList();
      // Auto-select the new run
      handleSelectRun(data.run_id);
    }
  }

  function handleJobOutput(data) {
    // Only render output if viewing this job AND this specific run
    if (state.selectedJobId && data.job_id === state.selectedJobId &&
        state.selectedRunId && data.run_id === state.selectedRunId) {
      var output = data.data || "";
      var lines = output.split("\n");
      for (var i = 0; i < lines.length; i++) {
        if (i === lines.length - 1 && lines[i] === "") continue;
        appendLogLineWithTimestamp(lines[i], data.timestamp, "");
      }
    }
  }

  function handleJobCompleted(data) {
    fetchJobs();

    if (state.selectedJobId && data.job_id === state.selectedJobId) {
      // Update the run in state.runs
      for (var i = 0; i < state.runs.length; i++) {
        if (state.runs[i].run_id === data.run_id) {
          state.runs[i].status = "Completed";
          state.runs[i].exit_code = data.exit_code;
          state.runs[i].finished_at = data.timestamp;
          break;
        }
      }
      renderRunsList();

      // Only show completion message if viewing this run
      if (state.selectedRunId === data.run_id) {
        appendLogLineWithTimestamp(
          "Job completed with exit code " + data.exit_code,
          data.timestamp,
          data.exit_code === 0 ? "log-completed" : "log-failed"
        );
      }
    }
  }

  function handleJobFailed(data) {
    fetchJobs();

    if (state.selectedJobId && data.job_id === state.selectedJobId) {
      // Update the run in state.runs
      for (var i = 0; i < state.runs.length; i++) {
        if (state.runs[i].run_id === data.run_id) {
          state.runs[i].status = "Failed";
          state.runs[i].finished_at = data.timestamp;
          break;
        }
      }
      renderRunsList();

      if (state.selectedRunId === data.run_id) {
        appendLogLineWithTimestamp(
          "Job failed: " + (data.error || "unknown error"),
          data.timestamp,
          "log-failed"
        );
      }
    }
  }

  function handleJobChanged(data) {
    // Refresh jobs on any change (add, update, remove, enable, disable)
    fetchJobs();

    var change = data.change || "unknown";
    if (change === "Removed" && state.selectedJobId === data.job_id) {
      appendLogLine("Job has been deleted.", "log-failed");
      closeSidebar();
    }
  }

  // -------------------------------------------------------------------
  // Execution type label update
  // -------------------------------------------------------------------

  function handleExecTypeChange() {
    if (dom.formExecType.value === "ScriptFile") {
      dom.formExecValueLabel.textContent = "Script File";
      dom.formExecValue.placeholder = "deploy.sh";
    } else {
      dom.formExecValueLabel.textContent = "Command";
      dom.formExecValue.placeholder = "echo hello";
    }
  }

  // -------------------------------------------------------------------
  // Keyboard Shortcuts
  // -------------------------------------------------------------------

  function handleKeydown(e) {
    if (e.key === "Escape") {
      // Check highest z-index modals first (delete/edit at 1000, sidebar at 201)
      if (dom.deleteOverlay.classList.contains("visible")) {
        closeDeleteModal();
      } else if (dom.modalOverlay.classList.contains("visible")) {
        closeModal();
      } else if (dom.sidebar.classList.contains("open")) {
        closeSidebar();
      }
    }
  }

  // -------------------------------------------------------------------
  // Polling Timers
  // -------------------------------------------------------------------

  function startHealthPolling() {
    fetchHealth();
    state.healthTimer = setInterval(fetchHealth, HEALTH_POLL_INTERVAL);
  }

  function startJobsPolling() {
    fetchJobs();
    // Fallback polling in case SSE is not connected
    state.jobsTimer = setInterval(function () {
      if (!state.sseConnected) {
        fetchJobs();
      }
    }, JOBS_POLL_INTERVAL);
  }

  // -------------------------------------------------------------------
  // Relative Time Auto-Refresh
  // -------------------------------------------------------------------

  function startRelativeTimeRefresh() {
    // Refresh relative times every 30 seconds by re-rendering
    setInterval(function () {
      if (state.jobs.length > 0) {
        renderJobTable();
      }
    }, 30000);
  }

  // -------------------------------------------------------------------
  // Initialization
  // -------------------------------------------------------------------

  function init() {
    cacheDom();

    // Event listeners — Toolbar
    dom.btnAddJob.addEventListener("click", openAddModal);

    // Event listeners — Job Form Modal
    dom.modalClose.addEventListener("click", closeModal);
    dom.btnCancelForm.addEventListener("click", closeModal);
    dom.jobForm.addEventListener("submit", handleFormSubmit);
    dom.formExecType.addEventListener("change", handleExecTypeChange);

    // Close modal when clicking overlay background
    dom.modalOverlay.addEventListener("click", function (e) {
      if (e.target === dom.modalOverlay) closeModal();
    });

    // Event listeners — Delete Modal
    dom.deleteModalClose.addEventListener("click", closeDeleteModal);
    dom.btnCancelDelete.addEventListener("click", closeDeleteModal);
    dom.btnConfirmDelete.addEventListener("click", handleConfirmDelete);
    dom.deleteOverlay.addEventListener("click", function (e) {
      if (e.target === dom.deleteOverlay) closeDeleteModal();
    });

    // Event listeners — Sidebar
    dom.btnCloseSidebar.addEventListener("click", closeSidebar);
    dom.sidebarBackdrop.addEventListener("click", closeSidebar);
    dom.btnClearLog.addEventListener("click", clearLog);
    dom.btnRefreshRuns.addEventListener("click", function () {
      if (state.selectedJobId) loadRunsList(state.selectedJobId);
    });
    dom.btnSidebarRun.addEventListener("click", function () {
      if (state.selectedJobId) handleTrigger(state.selectedJobId);
    });
    dom.btnSidebarEdit.addEventListener("click", function () {
      if (state.selectedJobId) {
        closeSidebar();
        handleEdit(state.selectedJobId);
      }
    });
    dom.btnSidebarDelete.addEventListener("click", function () {
      if (state.selectedJobId) {
        handleDeletePrompt(state.selectedJobId, state.selectedJobName);
      }
    });

    // Keyboard
    document.addEventListener("keydown", handleKeydown);

    // Start polling and SSE
    startHealthPolling();
    startJobsPolling();
    startRelativeTimeRefresh();
    connectSSE();
    lucide.createIcons();
  }

  // -------------------------------------------------------------------
  // Boot
  // -------------------------------------------------------------------

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
