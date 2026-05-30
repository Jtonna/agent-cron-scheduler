/**
 * timezones
 *
 * Hardcoded shortlist of common IANA timezone names used by the
 * timezone picker in the workflow editor's schedule card. Deliberately
 * small — the picker is a popover, not a full tz database browser. The
 * input remains free-text so users can paste any IANA zone the backend
 * (`chrono-tz`) supports.
 */

export const COMMON_TIMEZONES: string[] = [
  "UTC",
  "America/Los_Angeles",
  "America/Denver",
  "America/Chicago",
  "America/New_York",
  "America/Sao_Paulo",
  "Europe/London",
  "Europe/Berlin",
  "Europe/Paris",
  "Asia/Dubai",
  "Asia/Kolkata",
  "Asia/Singapore",
  "Asia/Tokyo",
  "Australia/Sydney",
  "Pacific/Auckland",
];
