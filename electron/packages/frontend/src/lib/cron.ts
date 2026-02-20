export const COMMON_TIMEZONES = [
  "UTC",
  "America/New_York",
  "America/Chicago",
  "America/Denver",
  "America/Los_Angeles",
  "America/Anchorage",
  "Pacific/Honolulu",
  "Europe/London",
  "Europe/Berlin",
  "Europe/Paris",
  "Europe/Moscow",
  "Asia/Tokyo",
  "Asia/Shanghai",
  "Asia/Kolkata",
  "Asia/Dubai",
  "Australia/Sydney",
  "Pacific/Auckland",
];

const DAY_NAMES = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
];

const MONTH_NAMES = [
  "", "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];

export interface CronFields {
  minute: string;
  hour: string;
  dayOfMonth: string;
  month: string;
  dayOfWeek: string;
}

export function parseCronFields(expression: string): CronFields {
  const parts = expression.trim().split(/\s+/);
  return {
    minute: parts[0] ?? "*",
    hour: parts[1] ?? "*",
    dayOfMonth: parts[2] ?? "*",
    month: parts[3] ?? "*",
    dayOfWeek: parts[4] ?? "*",
  };
}

export function fieldsToExpression(fields: CronFields): string {
  return `${fields.minute} ${fields.hour} ${fields.dayOfMonth} ${fields.month} ${fields.dayOfWeek}`;
}

export function validateCron(expression: string): string | null {
  const parts = expression.trim().split(/\s+/);
  if (parts.length !== 5) return "Must be a 5-field cron expression";

  const fieldValidators: [string, number, number][] = [
    ["Minute", 0, 59],
    ["Hour", 0, 23],
    ["Day of month", 1, 31],
    ["Month", 1, 12],
    ["Day of week", 0, 7],
  ];

  for (let i = 0; i < 5; i++) {
    const [name, min, max] = fieldValidators[i];
    const err = validateField(parts[i], name, min, max);
    if (err) return err;
  }

  return null;
}

function validateField(
  field: string, name: string, min: number, max: number
): string | null {
  if (field === "*") return null;

  // Handle lists: 1,3,5
  const listParts = field.split(",");
  for (const part of listParts) {
    const err = validateSingleOrRange(part, name, min, max);
    if (err) return err;
  }
  return null;
}

function validateSingleOrRange(
  part: string, name: string, min: number, max: number
): string | null {
  // Step: */N or N-M/S
  const stepMatch = part.match(/^(.+)\/(\d+)$/);
  if (stepMatch) {
    const step = parseInt(stepMatch[2], 10);
    if (step < 1) return `${name}: step must be >= 1`;
    const base = stepMatch[1];
    if (base === "*") return null;
    return validateRange(base, name, min, max);
  }

  // Range: N-M
  if (part.includes("-")) {
    return validateRange(part, name, min, max);
  }

  // Single value
  const val = parseInt(part, 10);
  if (isNaN(val) || val < min || val > max) {
    return `${name}: "${part}" is not valid (${min}-${max})`;
  }
  return null;
}

function validateRange(
  range: string, name: string, min: number, max: number
): string | null {
  const [startStr, endStr] = range.split("-");
  const start = parseInt(startStr, 10);
  const end = parseInt(endStr, 10);
  if (isNaN(start) || isNaN(end)) return `${name}: invalid range "${range}"`;
  if (start < min || start > max) return `${name}: ${start} is out of range (${min}-${max})`;
  if (end < min || end > max) return `${name}: ${end} is out of range (${min}-${max})`;
  return null;
}

/* ── Human-readable description ── */

export function cronToHuman(expression: string): string {
  const err = validateCron(expression);
  if (err) return err;

  const { minute, hour, dayOfMonth, month, dayOfWeek } = parseCronFields(expression);

  const parts: string[] = [];

  // Frequency / time part
  parts.push(describeFrequency(minute, hour));

  // When both day-of-month and day-of-week are set, cron uses OR logic
  const hasBothDayFields = dayOfMonth !== "*" && dayOfWeek !== "*";

  if (hasBothDayFields) {
    parts.push(
      `on ${describeDayOfMonth(dayOfMonth)} OR any ${describeDaysOfWeek(dayOfWeek)}`
    );
  } else {
    if (dayOfMonth !== "*") {
      parts.push(`on ${describeDayOfMonth(dayOfMonth)}`);
    }
    if (dayOfWeek !== "*") {
      parts.push(`on ${describeDaysOfWeek(dayOfWeek)}`);
    }
  }

  // Month constraint
  if (month !== "*") {
    parts.push(`in ${describeMonths(month)}`);
  }

  return parts.join(", ");
}

function describeMinute(minute: string): string {
  if (minute === "*") return "every minute";
  if (minute.startsWith("*/")) {
    const n = minute.slice(2);
    return n === "1" ? "every minute" : `every ${n} minutes`;
  }
  return `at minute ${minute.padStart(2, "0")}`;
}

function describeHour(hour: string): string {
  if (hour === "*") return "";
  if (hour.startsWith("*/")) {
    const n = hour.slice(2);
    return n === "1" ? "every hour" : `every ${n}${ordinal(n)} hour`;
  }
  if (/^\d+$/.test(hour)) return `during the ${hour.padStart(2, "0")}:00 hour`;
  if (hour.includes("-")) {
    const [s, e] = hour.split("-");
    return `between hours ${s.padStart(2, "0")} and ${e.padStart(2, "0")}`;
  }
  return `during hours ${hour}`;
}

function ordinal(n: string): string {
  const num = parseInt(n, 10);
  if (num === 1) return "st";
  if (num === 2) return "nd";
  if (num === 3) return "rd";
  return "th";
}

function describeFrequency(minute: string, hour: string): string {
  // Simple: specific time
  if (/^\d+$/.test(minute) && /^\d+$/.test(hour)) {
    return `Runs at ${hour.padStart(2, "0")}:${minute.padStart(2, "0")}`;
  }

  const minPart = describeMinute(minute);
  const hourPart = describeHour(hour);

  if (hourPart) {
    return `Runs ${minPart}, ${hourPart}`;
  }
  return `Runs ${minPart}`;
}

function describeDayOfMonth(field: string): string {
  return field.split(",").map((p) => {
    if (p.startsWith("*/")) return `every ${p.slice(2)}${ordinal(p.slice(2))} day`;
    if (p.includes("/")) {
      const [base, step] = p.split("/");
      return `every ${step}${ordinal(step)} day starting from day ${base}`;
    }
    if (p.includes("-")) return `days ${p}`;
    return `day ${p}`;
  }).join(", ");
}

function describeMonths(field: string): string {
  return field.split(",").map((p) => {
    if (p.startsWith("*/")) return `every ${p.slice(2)}${ordinal(p.slice(2))} month`;
    if (p.includes("-")) {
      const [s, e] = p.split("-").map(Number);
      return `${MONTH_NAMES[s] || s} through ${MONTH_NAMES[e] || e}`;
    }
    const n = parseInt(p, 10);
    return MONTH_NAMES[n] || p;
  }).join(", ");
}

function describeDaysOfWeek(field: string): string {
  if (field === "1-5") return "weekdays";
  if (field === "0,6" || field === "6,0") return "weekends";

  return field.split(",").map((p) => {
    if (p.startsWith("*/")) return `every ${p.slice(2)}${ordinal(p.slice(2))} day of the week`;
    if (p.includes("-")) {
      const [s, e] = p.split("-").map(Number);
      return `${DAY_NAMES[s] || s} through ${DAY_NAMES[e % 7] || e}`;
    }
    const n = parseInt(p, 10);
    if (!isNaN(n)) return DAY_NAMES[n % 7] || p;
    return p;
  }).join(" and ");
}
