export function fmtSize(n: number): string {
  if (!n || n <= 0) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

export function fmtSpeed(bps: number): string {
  if (!bps || bps <= 0) return "";
  return `${fmtSize(bps)}/s`;
}

export function pluralKey(n: number, lang: string): "One" | "Few" | "Many" {
  if (lang !== "ru") return n === 1 ? "One" : "Few";
  const m10 = Math.abs(n) % 10;
  const m100 = Math.abs(n) % 100;
  if (m10 === 1 && m100 !== 11) return "One";
  if (m10 >= 2 && m10 <= 4 && (m100 < 12 || m100 > 14)) return "Few";
  return "Many";
}

export function fmtUptime(totalSecs: number | null | undefined, lang: string): string {
  if (totalSecs == null || totalSecs < 0) return "—";
  const d = Math.floor(totalSecs / 86400);
  const h = Math.floor((totalSecs % 86400) / 3600);
  const m = Math.floor((totalSecs % 3600) / 60);
  const s = Math.floor(totalSecs % 60);
  const dd = lang === "ru" ? "д" : "d";
  const hh = lang === "ru" ? "ч" : "h";
  const mm = lang === "ru" ? "мин" : "m";
  const ss = lang === "ru" ? "с" : "s";
  if (d > 0) return `${d}${dd} ${h}${hh}`;
  if (h > 0) return `${h}${hh} ${m}${mm}`;
  if (m > 0) return `${m}${mm} ${s}${ss}`;
  return `${s}${ss}`;
}

export function fmtDate(ts: number | null | undefined, lang: string): string {
  if (!ts) return "—";
  try {
    const d = new Date(ts * 1000);
    return d.toLocaleString(lang === "ru" ? "ru-RU" : "en-US", {
      day: "2-digit",
      month: "2-digit",
      year: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "—";
  }
}

export function fmtLastUsed(iso: string | null | undefined, lang: string): string {
  if (!iso) return "";
  try {
    const d = new Date(iso);
    return d.toLocaleString(lang === "ru" ? "ru-RU" : "en-US", {
      day: "2-digit",
      month: "2-digit",
      year: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "";
  }
}

export function remoteJoin(dir: string, name: string): string {
  if (dir === "/") return `/${name}`;
  return `${dir.replace(/\/+$/, "")}/${name}`;
}

// Path bar uses "~" as an alias for the server root "/".
export function displayPath(real: string): string {
  if (real === "/") return "~/";
  if (real.startsWith("/")) return `~${real}`;
  return real;
}

export function resolveInputPath(input: string): string {
  const t = input.trim();
  if (t === "~" || t === "~/") return "/";
  if (t.startsWith("~/")) return `/${t.slice(2)}`;
  return t;
}

export function remoteParent(p: string): string {
  if (!p || p === "/") return "/";
  const t = p.replace(/\/+$/, "");
  const i = t.lastIndexOf("/");
  if (i <= 0) return "/";
  return t.slice(0, i);
}

export function isValidName(name: string): boolean {
  const t = name.trim();
  if (!t || t === "." || t === "..") return false;
  return !/[/\\]/.test(t);
}
