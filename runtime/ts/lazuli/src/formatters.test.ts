import { describe, expect, it } from "vitest";

import {
  formatBrazilianCNPJ,
  formatBrazilianCPF,
  formatBrazilianPhone,
  formatCurrency,
  formatDate,
  formatMoney,
  formatPhone,
  parseBrazilianCNPJInput,
  parseBrazilianCPFInput,
  parseBrazilianPhoneInput,
  parseCurrencyInput,
  parseDateInput,
  parseMoneyInput,
  parsePhoneInput,
} from "./formatters.js";

function normalizeSpaces(value: string): string {
  return value.replace(/\u00a0/g, " ");
}

describe("semantic formatters", () => {
  it("formats and parses money without throwing on invalid input", () => {
    expect(normalizeSpaces(formatMoney(1234.56, "pt-BR"))).toBe("R$ 1.234,56");
    expect(parseMoneyInput("R$ 1.234,56", "pt-BR")).toBe(1234.56);
    expect(parseMoneyInput("not money", "pt-BR")).toBeNull();
    expect(normalizeSpaces(formatMoney({ amount: "1234.56", currency: "USD" }, "en-US"))).toBe(
      "$1,234.56",
    );
  });

  it("formats and parses date input", () => {
    expect(formatDate("2026-05-22", "pt-BR")).toBe("22/05/2026");
    expect(dateKey(parseDateInput("22/05/2026", "pt-BR"))).toBe("2026-05-22");
    expect(dateKey(parseDateInput("05/22/2026", "en-US"))).toBe("2026-05-22");
    expect(parseDateInput("31/02/2026", "pt-BR")).toBeNull();
  });

  it("formats and parses generic phone input best-effort", () => {
    expect(formatPhone("+15551234567")).toBe("+1 (555) 123-4567");
    expect(parsePhoneInput("+1 (555) 123-4567")).toBe("+15551234567");
    expect(parsePhoneInput("abc")).toBeNull();
  });

  it("formats and parses Brazilian phone input", () => {
    expect(formatBrazilianPhone("48999876543")).toBe("(48) 99987-6543");
    expect(formatBrazilianPhone("4833216543")).toBe("(48) 3321-6543");
    expect(parseBrazilianPhoneInput("+55 (48) 99987-6543")).toBe("48999876543");
    expect(formatBrazilianPhone("123")).toBe("123");
    expect(parseBrazilianPhoneInput("123")).toBeNull();
  });

  it("formats and parses Brazilian CPF input", () => {
    expect(formatBrazilianCPF("12345678901")).toBe("123.456.789-01");
    expect(parseBrazilianCPFInput("123.456.789-01")).toBe("12345678901");
    expect(formatBrazilianCPF("123")).toBe("123");
    expect(parseBrazilianCPFInput("123")).toBeNull();
  });

  it("formats and parses Brazilian CNPJ input", () => {
    expect(formatBrazilianCNPJ("12345678000199")).toBe("12.345.678/0001-99");
    expect(parseBrazilianCNPJInput("12.345.678/0001-99")).toBe("12345678000199");
    expect(formatBrazilianCNPJ("123")).toBe("123");
    expect(parseBrazilianCNPJInput("123")).toBeNull();
  });

  it("formats and parses ISO currency codes", () => {
    expect(formatCurrency("brl")).toBe("BRL");
    expect(parseCurrencyInput(" usd ")).toBe("USD");
    expect(formatCurrency("bitcoin")).toBe("bitcoin");
    expect(parseCurrencyInput("bitcoin")).toBeNull();
  });
});

function dateKey(value: Date | null): string | null {
  if (!value) return null;
  const year = value.getFullYear().toString().padStart(4, "0");
  const month = (value.getMonth() + 1).toString().padStart(2, "0");
  const day = value.getDate().toString().padStart(2, "0");
  return `${year}-${month}-${day}`;
}
