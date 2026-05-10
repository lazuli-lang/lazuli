// End-to-end smoke test: drive the Go runtime through the typed customer
// client. Run while the Go server (`dist/go`) is listening, e.g.:
//
//   LAZULI_ADDR=:8097 go run . &
//   tsx smoke.mts
//
// Verifies:
//   - LazuliClient.runCommand creates a row and returns the typed Customer
//   - LazuliClient.runQuery returns Customer[]
//   - The same client can drive the lookup query for a typed single row
//   - LazuliError is thrown for not-found lookups (with code "not_found")

import { LazuliClient, isLazuliError } from "@lazuli/runtime";
import {
  archiveCustomer,
  createCustomer,
  customerByID,
  listCustomers,
  type Customer,
} from "@lazuli/dist-customer";

const baseUrl = process.env.LAZULI_BASE_URL ?? "http://localhost:8097";
const client = new LazuliClient({
  baseUrl,
  headers: {
    "X-Lazuli-Actor": "user",
    "X-Lazuli-User-ID": "1",
    "X-Lazuli-Org-ID": "42",
    "X-Lazuli-Roles": "admin",
  },
});

function expect(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new Error(`EXPECT FAILED: ${message}`);
  }
}

async function main() {
  const stamp = Date.now();
  const email = `smoke-${stamp}@web.example`;

  console.log(`[smoke] base=${baseUrl}`);

  const created = await client.runCommand(createCustomer, {
    name: "Web Smoke",
    email,
  });
  expect(typeof created.id === "number", "create returned numeric id");
  expect(created.email === email, "create echoed email");
  console.log(`[smoke] created customer id=${created.id} email=${created.email}`);

  const list: Customer[] = await client.runQuery(listCustomers, {});
  expect(Array.isArray(list), "list returns array");
  expect(list.some((c) => c.id === created.id), "list contains created row");
  console.log(`[smoke] list returned ${list.length} customers`);

  const filtered = await client.runQuery(listCustomers, { search: "smoke" });
  expect(filtered.some((c) => c.id === created.id), "search returns the row");
  console.log(`[smoke] search 'smoke' returned ${filtered.length} customers`);

  const single = await client.runQuery(customerByID, { id: created.id });
  expect(single.id === created.id, "lookup returns the same row");
  console.log(`[smoke] by_id resolved to id=${single.id}`);

  // archive then verify lookup returns 404.
  await client.runCommand(archiveCustomer, { ID: created.id });
  console.log(`[smoke] archived id=${created.id}`);

  let notFoundCode: string | null = null;
  try {
    await client.runQuery(customerByID, { id: created.id });
  } catch (err) {
    if (!isLazuliError(err)) throw err;
    notFoundCode = err.code;
  }
  expect(notFoundCode === "not_found", `expected not_found, got ${notFoundCode}`);
  console.log(`[smoke] archived row no longer found (code=${notFoundCode})`);

  console.log("[smoke] OK");
}

main().catch((err) => {
  console.error("[smoke] FAILED");
  if (isLazuliError(err)) {
    console.error("  status:", err.status, "code:", err.code, "message:", err.message);
  } else {
    console.error(err);
  }
  process.exitCode = 1;
});
