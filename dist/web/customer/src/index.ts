// `@lazuli/dist-customer` — typed client for the `customer` feature.
//
// Pair this entry with `@lazuli/dist-customer/react` for TanStack Query
// hooks. The non-react entry stays React-free so server-side scripts and
// edge workers can import it without paying the React cost.

export {
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

export { customerLabel } from "./extensions.js";
