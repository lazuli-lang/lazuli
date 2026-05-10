// React entrypoint for the typed customer client. Each hook is a thin,
// typed wrapper over `@lazuli/runtime/react`'s generic
// `useLazuliQuery` / `useLazuliCommand`. Generated code emits one of these
// per command and query so consumers get autocomplete on names, args, and
// return shapes without ever spelling a string.

import {
  useLazuliCommand,
  useLazuliQuery,
  type UseLazuliCommandOptions,
  type UseLazuliQueryOptions,
} from "@lazuli/runtime/react";

import {
  archiveCustomer,
  createCustomer,
  customerByID,
  listCustomers,
  updateCustomerEmail,
  type ArchiveCustomerInput,
  type CreateCustomerInput,
  type Customer,
  type CustomerByIDArgs,
  type ListCustomersArgs,
  type UpdateCustomerEmailInput,
} from "./customer.gen.js";

// ----------------------------------------------------------------------------
// Queries
// ----------------------------------------------------------------------------

export function useListCustomers(
  args: ListCustomersArgs = {},
  options: UseLazuliQueryOptions<ListCustomersArgs, Customer[]> = {},
) {
  return useLazuliQuery(listCustomers, args, options);
}

export function useCustomerByID(
  args: CustomerByIDArgs,
  options: UseLazuliQueryOptions<CustomerByIDArgs, Customer> = {},
) {
  return useLazuliQuery(customerByID, args, options);
}

// ----------------------------------------------------------------------------
// Commands
// ----------------------------------------------------------------------------

export function useCreateCustomer(
  options: UseLazuliCommandOptions<CreateCustomerInput, Customer> = {},
) {
  return useLazuliCommand(createCustomer, options);
}

export function useUpdateCustomerEmail(
  options: UseLazuliCommandOptions<UpdateCustomerEmailInput, Customer> = {},
) {
  return useLazuliCommand(updateCustomerEmail, options);
}

export function useArchiveCustomer(
  options: UseLazuliCommandOptions<ArchiveCustomerInput, Customer> = {},
) {
  return useLazuliCommand(archiveCustomer, options);
}
