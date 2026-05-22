const DEFAULT_LOCALE = "pt-BR";
const DEFAULT_CURRENCY = "BRL";

export interface MoneyInput {
  amount: string | number;
  currency: string;
}

export function formatMoney(
  value: number | string | MoneyInput,
  locale = DEFAULT_LOCALE,
  currency = DEFAULT_CURRENCY,
): string {
  const money = moneyParts(value, currency);
  const amount = Number(money.amount);
  if (!Number.isFinite(amount)) {
    return typeof value === "object" && value !== null
      ? `${money.currency} ${money.amount}`
      : String(value);
  }

  const isoCurrency = parseCurrencyInput(money.currency) ?? DEFAULT_CURRENCY;
  try {
    return new Intl.NumberFormat(locale, {
      style: "currency",
      currency: isoCurrency,
    }).format(amount);
  } catch {
    return new Intl.NumberFormat(DEFAULT_LOCALE, {
      style: "currency",
      currency: isoCurrency,
    }).format(amount);
  }
}

export function parseMoneyInput(input: string, locale = DEFAULT_LOCALE): number | null {
  return parseLocalizedNumber(input, locale);
}

export function formatDate(value: Date | string, locale = DEFAULT_LOCALE): string {
  const parsed = parseDateValue(value);
  if (!parsed) {
    return String(value);
  }
  try {
    return new Intl.DateTimeFormat(locale).format(parsed);
  } catch {
    return new Intl.DateTimeFormat(DEFAULT_LOCALE).format(parsed);
  }
}

export function parseDateInput(input: string, locale = DEFAULT_LOCALE): Date | null {
  const trimmed = input.trim();
  if (!trimmed) return null;

  const iso = /^(\d{4})-(\d{2})-(\d{2})$/.exec(trimmed);
  if (iso) {
    return buildDate(Number(iso[1] ?? ""), Number(iso[2] ?? ""), Number(iso[3] ?? ""));
  }

  const localized = /^(\d{1,2})[./-](\d{1,2})[./-](\d{2}|\d{4})$/.exec(trimmed);
  if (localized) {
    const first = Number(localized[1] ?? "");
    const second = Number(localized[2] ?? "");
    const year = normalizeYear(Number(localized[3] ?? ""));
    const monthFirst = /^en([-_])?US$/i.test(locale);
    return monthFirst ? buildDate(year, first, second) : buildDate(year, second, first);
  }

  const fallback = new Date(trimmed);
  return isValidDate(fallback) ? fallback : null;
}

export function formatPhone(value: string): string {
  const parsed = parsePhoneInput(value);
  if (!parsed) return value;

  const digits = parsed.replace(/^\+/, "");
  if (digits.startsWith("55") && (digits.length === 12 || digits.length === 13)) {
    const local = digits.slice(2);
    const formatted = formatBrazilianPhone(local);
    return formatted === local ? value : `+55 ${formatted}`;
  }
  if (digits.startsWith("1") && digits.length === 11) {
    return `+1 (${digits.slice(1, 4)}) ${digits.slice(4, 7)}-${digits.slice(7)}`;
  }

  return parsed.startsWith("+") ? `+${digits}` : value;
}

export function parsePhoneInput(input: string): string | null {
  const trimmed = input.trim();
  const digits = digitsOnly(trimmed);
  if (digits.length < 7 || digits.length > 15) return null;
  return trimmed.startsWith("+") ? `+${digits}` : digits;
}

export function formatBrazilianPhone(value: string): string {
  const digits = parseBrazilianPhoneInput(value);
  if (!digits) return value;
  if (digits.length === 11) {
    return `(${digits.slice(0, 2)}) ${digits.slice(2, 7)}-${digits.slice(7)}`;
  }
  return `(${digits.slice(0, 2)}) ${digits.slice(2, 6)}-${digits.slice(6)}`;
}

export function parseBrazilianPhoneInput(input: string): string | null {
  let digits = digitsOnly(input);
  if (digits.startsWith("55") && (digits.length === 12 || digits.length === 13)) {
    digits = digits.slice(2);
  }
  return digits.length === 10 || digits.length === 11 ? digits : null;
}

export function formatBrazilianCPF(value: string): string {
  const digits = parseBrazilianCPFInput(value);
  if (!digits) return value;
  return `${digits.slice(0, 3)}.${digits.slice(3, 6)}.${digits.slice(6, 9)}-${digits.slice(9)}`;
}

export function parseBrazilianCPFInput(input: string): string | null {
  const digits = digitsOnly(input);
  return digits.length === 11 ? digits : null;
}

export function formatBrazilianCNPJ(value: string): string {
  const digits = parseBrazilianCNPJInput(value);
  if (!digits) return value;
  return `${digits.slice(0, 2)}.${digits.slice(2, 5)}.${digits.slice(5, 8)}/${digits.slice(8, 12)}-${digits.slice(12)}`;
}

export function parseBrazilianCNPJInput(input: string): string | null {
  const digits = digitsOnly(input);
  return digits.length === 14 ? digits : null;
}

export function formatCurrency(value: string): string {
  return parseCurrencyInput(value) ?? value;
}

export function parseCurrencyInput(input: string): string | null {
  const normalized = input.trim().toUpperCase();
  return /^[A-Z]{3}$/.test(normalized) ? normalized : null;
}

function moneyParts(value: number | string | MoneyInput, currency: string): MoneyInput {
  if (typeof value === "object" && value !== null) {
    return {
      amount: value.amount,
      currency: value.currency,
    };
  }
  return { amount: value, currency };
}

function parseLocalizedNumber(input: string, locale: string): number | null {
  const trimmed = input.trim();
  if (!trimmed) return null;

  const separators = numberSeparators(locale);
  let normalized = trimmed.replace(/\u00a0/g, " ");
  const negative = /[-\u2212]/.test(normalized) || /^\(.*\)$/.test(normalized);
  normalized = normalized.replace(/[^\d.,]/g, "");
  if (!/\d/.test(normalized)) return null;

  if (separators.group) {
    normalized = replaceAll(normalized, separators.group, "");
  }
  if (separators.decimal && separators.decimal !== ".") {
    normalized = replaceAll(normalized, separators.decimal, ".");
  }

  const decimalCount = (normalized.match(/\./g) ?? []).length;
  if (decimalCount > 1) return null;

  const parsed = Number(normalized);
  if (!Number.isFinite(parsed)) return null;
  return negative ? -parsed : parsed;
}

function numberSeparators(locale: string): { decimal: string; group: string } {
  try {
    const parts = new Intl.NumberFormat(locale).formatToParts(12345.6);
    return {
      decimal: parts.find((part) => part.type === "decimal")?.value ?? ".",
      group: parts.find((part) => part.type === "group")?.value ?? ",",
    };
  } catch {
    return { decimal: ",", group: "." };
  }
}

function parseDateValue(value: Date | string): Date | null {
  if (value instanceof Date) {
    return isValidDate(value) ? value : null;
  }
  const trimmed = value.trim();
  const isoDate = /^(\d{4})-(\d{2})-(\d{2})$/.exec(trimmed);
  if (isoDate) {
    return buildDate(
      Number(isoDate[1] ?? ""),
      Number(isoDate[2] ?? ""),
      Number(isoDate[3] ?? ""),
    );
  }
  const parsed = new Date(trimmed);
  return isValidDate(parsed) ? parsed : null;
}

function normalizeYear(year: number): number {
  if (year < 100) {
    return year >= 70 ? 1900 + year : 2000 + year;
  }
  return year;
}

function buildDate(year: number, month: number, day: number): Date | null {
  if (!Number.isInteger(year) || !Number.isInteger(month) || !Number.isInteger(day)) {
    return null;
  }
  const date = new Date(year, month - 1, day);
  if (
    date.getFullYear() !== year ||
    date.getMonth() !== month - 1 ||
    date.getDate() !== day
  ) {
    return null;
  }
  return date;
}

function isValidDate(value: Date): boolean {
  return !Number.isNaN(value.getTime());
}

function digitsOnly(value: string): string {
  return value.replace(/\D/g, "");
}

function replaceAll(value: string, search: string, replacement: string): string {
  return search === "" ? value : value.split(search).join(replacement);
}
