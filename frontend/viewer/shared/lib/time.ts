const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** Human-readable relative time. `now` is injected for deterministic tests. */
export function relativeTime(iso: string, now: Date): string {
  const then = new Date(iso);
  if (Number.isNaN(then.getTime())) return "";
  const secs = Math.floor((now.getTime() - then.getTime()) / 1000);
  if (secs < 60) return "just now";
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days === 1) return "yesterday";
  if (days < 7) return `${days}d ago`;
  const month = MONTHS[then.getUTCMonth()];
  const day = then.getUTCDate();
  return then.getUTCFullYear() === now.getUTCFullYear()
    ? `${month} ${day}`
    : `${month} ${day}, ${then.getUTCFullYear()}`;
}
