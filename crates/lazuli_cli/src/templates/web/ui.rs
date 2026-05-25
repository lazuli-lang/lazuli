//! Frontend web `ui/*` scaffold seeds (Wave K — W2 Shadcn-seed
//! primitives shipped 2026-05-17). One essential primitive per `ui/`
//! kind (Button / Input / Toast / Card / Dialog / Stack) + the
//! `cn()` helper, each emitted with a scaffold-seed banner so users
//! know they own the file and Lazuli will never overwrite it on
//! re-scaffold (idempotency guard in
//! `cmd_new_frontends::write_if_absent`).
//!
//! Closed-set discipline (do NOT expand here without a fresh grading
//! cycle): v0 = 6 primitives. Pilots may need more for v0.2; that's a
//! separate proposal.
//!
//! Wave R7-3 extract: lifted out of `templates/web.rs`.

// ---------------------------------------------------------------
// Wave K — W2 Shadcn-seed primitives shipped 2026-05-17.
//
// Per W2 grading (anchor: docs/proposals/
// lazurite-frontend-stack-web-grading-2026-05-17.md §W2), the design-
// system pick is "Radix Primitives + Tailwind, with Shadcn-compose
// toolkit (CVA + clsx + tailwind-merge) as deps, Shadcn primitives
// copy-pasted into the scaffold as SEED FILES THE PRODUCT OWNS".
//
// The constants below ship the v0 closed set: ONE essential primitive
// per `ui/` kind (6 total) + the `cn()` helper. Each emits with a
// scaffold-seed banner so users know they own the file and Lazuli will
// never overwrite it on re-scaffold (idempotency guard in
// `cmd_new_frontends::write_if_absent`).
//
// Closed-set discipline (do NOT expand here without a fresh grading
// cycle): v0 = 6 primitives. Pilots may need more for v0.2; that's a
// separate proposal.
// ---------------------------------------------------------------

/// `cn()` helper used by every CVA-driven primitive. Standard Shadcn
/// recipe — `clsx` for conditional composition + `tailwind-merge` for
/// last-write-wins conflict resolution between Tailwind utility classes.
/// User-owned scaffold seed.
pub const FRONTEND_WEB_THEME_CN_TS: &str = r#"// Scaffold seed (Wave K 2026-05-17). User owns this file. Edit freely.
// Lazuli never overwrites.
import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * Compose Tailwind class strings with conditional support and conflict
 * resolution. `cn("px-2 px-4")` -> `"px-4"`.
 */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
"#;

/// `ui/forms/Button.tsx` — CVA variants (default/destructive/outline/
/// ghost) + sizes (sm/md/lg/icon) + Radix `Slot` for `asChild`.
pub const FRONTEND_WEB_UI_BUTTON_TSX: &str = r#"// Scaffold seed (Wave K 2026-05-17). User owns this file. Edit freely.
// Lazuli never overwrites.
import { Slot } from "@radix-ui/react-slot";
import { type VariantProps, cva } from "class-variance-authority";
import { forwardRef } from "react";
import type { ButtonHTMLAttributes } from "react";

import { cn } from "@web/theme/cn";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50",
  {
    variants: {
      variant: {
        default:
          "bg-primary text-primary-foreground hover:bg-primary/90",
        destructive:
          "bg-destructive text-destructive-foreground hover:bg-destructive/90",
        outline:
          "border border-input bg-background hover:bg-accent hover:text-accent-foreground",
        ghost: "hover:bg-accent hover:text-accent-foreground",
      },
      size: {
        sm: "h-8 px-3",
        md: "h-10 px-4 py-2",
        lg: "h-11 px-8",
        icon: "h-10 w-10",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "md",
    },
  },
);

export interface ButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return (
      <Comp
        className={cn(buttonVariants({ variant, size, className }))}
        ref={ref}
        {...props}
      />
    );
  },
);
Button.displayName = "Button";

export { buttonVariants };
"#;

/// `ui/forms/Input.tsx` — closed-set props mirroring native + Tailwind
/// variants. Refs forwarded.
pub const FRONTEND_WEB_UI_INPUT_TSX: &str = r#"// Scaffold seed (Wave K 2026-05-17). User owns this file. Edit freely.
// Lazuli never overwrites.
import { forwardRef } from "react";
import type { InputHTMLAttributes } from "react";

import { cn } from "@web/theme/cn";

export type InputProps = InputHTMLAttributes<HTMLInputElement>;

export const Input = forwardRef<HTMLInputElement, InputProps>(
  ({ className, type, ...props }, ref) => {
    return (
      <input
        type={type}
        className={cn(
          "flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50",
          className,
        )}
        ref={ref}
        {...props}
      />
    );
  },
);
Input.displayName = "Input";
"#;

/// `ui/feedback/Toast.tsx` — Sonner wrapper. Re-exports `Toaster` for
/// shell mount + `toast()` API for call sites.
pub const FRONTEND_WEB_UI_TOAST_TSX: &str = r#"// Scaffold seed (Wave K 2026-05-17). User owns this file. Edit freely.
// Lazuli never overwrites.
import { Toaster as SonnerToaster, toast } from "sonner";
import type { ComponentProps } from "react";

type ToasterProps = ComponentProps<typeof SonnerToaster>;

/**
 * App-shell Toaster. Mount once at the root (e.g. inside `shell/root.tsx`):
 *
 *     <Toaster richColors />
 *
 * Call sites use the re-exported `toast` API:
 *
 *     toast.success("Saved");
 *     toast.error("Something went wrong");
 */
export function Toaster(props: ToasterProps) {
  return <SonnerToaster richColors closeButton {...props} />;
}

export { toast };
"#;

/// `ui/display/Card.tsx` — Card + CardHeader + CardTitle +
/// CardDescription + CardContent + CardFooter sub-components per
/// Shadcn idiom.
pub const FRONTEND_WEB_UI_CARD_TSX: &str = r#"// Scaffold seed (Wave K 2026-05-17). User owns this file. Edit freely.
// Lazuli never overwrites.
import { forwardRef } from "react";
import type { HTMLAttributes } from "react";

import { cn } from "@web/theme/cn";

export const Card = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>(
  ({ className, ...props }, ref) => (
    <div
      ref={ref}
      className={cn(
        "rounded-lg border bg-card text-card-foreground shadow-sm",
        className,
      )}
      {...props}
    />
  ),
);
Card.displayName = "Card";

export const CardHeader = forwardRef<
  HTMLDivElement,
  HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    className={cn("flex flex-col space-y-1.5 p-6", className)}
    {...props}
  />
));
CardHeader.displayName = "CardHeader";

export const CardTitle = forwardRef<
  HTMLDivElement,
  HTMLAttributes<HTMLHeadingElement>
>(({ className, ...props }, ref) => (
  <h3
    ref={ref}
    className={cn(
      "text-2xl font-semibold leading-none tracking-tight",
      className,
    )}
    {...props}
  />
));
CardTitle.displayName = "CardTitle";

export const CardDescription = forwardRef<
  HTMLDivElement,
  HTMLAttributes<HTMLParagraphElement>
>(({ className, ...props }, ref) => (
  <p
    ref={ref}
    className={cn("text-sm text-muted-foreground", className)}
    {...props}
  />
));
CardDescription.displayName = "CardDescription";

export const CardContent = forwardRef<
  HTMLDivElement,
  HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div ref={ref} className={cn("p-6 pt-0", className)} {...props} />
));
CardContent.displayName = "CardContent";

export const CardFooter = forwardRef<
  HTMLDivElement,
  HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    className={cn("flex items-center p-6 pt-0", className)}
    {...props}
  />
));
CardFooter.displayName = "CardFooter";
"#;

/// `ui/overlays/Dialog.tsx` — Radix `@radix-ui/react-dialog` wrapper.
/// Closed-set sub-components per Shadcn idiom: Dialog, DialogTrigger,
/// DialogContent, DialogHeader, DialogFooter, DialogTitle,
/// DialogDescription, DialogClose.
pub const FRONTEND_WEB_UI_DIALOG_TSX: &str = r#"// Scaffold seed (Wave K 2026-05-17). User owns this file. Edit freely.
// Lazuli never overwrites.
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import { forwardRef } from "react";
import type { ComponentPropsWithoutRef, ElementRef, HTMLAttributes } from "react";

import { cn } from "@web/theme/cn";

export const Dialog = DialogPrimitive.Root;
export const DialogTrigger = DialogPrimitive.Trigger;
export const DialogPortal = DialogPrimitive.Portal;
export const DialogClose = DialogPrimitive.Close;

export const DialogOverlay = forwardRef<
  ElementRef<typeof DialogPrimitive.Overlay>,
  ComponentPropsWithoutRef<typeof DialogPrimitive.Overlay>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Overlay
    ref={ref}
    className={cn(
      "fixed inset-0 z-50 bg-black/80 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0",
      className,
    )}
    {...props}
  />
));
DialogOverlay.displayName = DialogPrimitive.Overlay.displayName;

export const DialogContent = forwardRef<
  ElementRef<typeof DialogPrimitive.Content>,
  ComponentPropsWithoutRef<typeof DialogPrimitive.Content>
>(({ className, children, ...props }, ref) => (
  <DialogPortal>
    <DialogOverlay />
    <DialogPrimitive.Content
      ref={ref}
      className={cn(
        "fixed left-[50%] top-[50%] z-50 grid w-full max-w-lg translate-x-[-50%] translate-y-[-50%] gap-4 border bg-background p-6 shadow-lg duration-200 sm:rounded-lg",
        className,
      )}
      {...props}
    >
      {children}
      <DialogPrimitive.Close className="absolute right-4 top-4 rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:pointer-events-none">
        <X className="h-4 w-4" />
        <span className="sr-only">Close</span>
      </DialogPrimitive.Close>
    </DialogPrimitive.Content>
  </DialogPortal>
));
DialogContent.displayName = DialogPrimitive.Content.displayName;

export function DialogHeader({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        "flex flex-col space-y-1.5 text-center sm:text-left",
        className,
      )}
      {...props}
    />
  );
}
DialogHeader.displayName = "DialogHeader";

export function DialogFooter({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        "flex flex-col-reverse sm:flex-row sm:justify-end sm:space-x-2",
        className,
      )}
      {...props}
    />
  );
}
DialogFooter.displayName = "DialogFooter";

export const DialogTitle = forwardRef<
  ElementRef<typeof DialogPrimitive.Title>,
  ComponentPropsWithoutRef<typeof DialogPrimitive.Title>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Title
    ref={ref}
    className={cn(
      "text-lg font-semibold leading-none tracking-tight",
      className,
    )}
    {...props}
  />
));
DialogTitle.displayName = DialogPrimitive.Title.displayName;

export const DialogDescription = forwardRef<
  ElementRef<typeof DialogPrimitive.Description>,
  ComponentPropsWithoutRef<typeof DialogPrimitive.Description>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Description
    ref={ref}
    className={cn("text-sm text-muted-foreground", className)}
    {...props}
  />
));
DialogDescription.displayName = DialogPrimitive.Description.displayName;
"#;

/// `ui/layout/Stack.tsx` — flex layout with gap variants. Pure Tailwind,
/// no Radix needed. Closed-set props: `direction`, `gap`, `align`,
/// `justify`, `wrap`.
pub const FRONTEND_WEB_UI_STACK_TSX: &str = r#"// Scaffold seed (Wave K 2026-05-17). User owns this file. Edit freely.
// Lazuli never overwrites.
import { type VariantProps, cva } from "class-variance-authority";
import { forwardRef } from "react";
import type { HTMLAttributes } from "react";

import { cn } from "@web/theme/cn";

const stackVariants = cva("flex", {
  variants: {
    direction: {
      row: "flex-row",
      col: "flex-col",
    },
    gap: {
      none: "gap-0",
      sm: "gap-2",
      md: "gap-4",
      lg: "gap-6",
      xl: "gap-8",
    },
    align: {
      start: "items-start",
      center: "items-center",
      end: "items-end",
      stretch: "items-stretch",
    },
    justify: {
      start: "justify-start",
      center: "justify-center",
      end: "justify-end",
      between: "justify-between",
      around: "justify-around",
    },
    wrap: {
      true: "flex-wrap",
      false: "flex-nowrap",
    },
  },
  defaultVariants: {
    direction: "col",
    gap: "md",
    align: "stretch",
    justify: "start",
    wrap: false,
  },
});

export interface StackProps
  extends HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof stackVariants> {}

export const Stack = forwardRef<HTMLDivElement, StackProps>(
  ({ className, direction, gap, align, justify, wrap, ...props }, ref) => (
    <div
      ref={ref}
      className={cn(
        stackVariants({ direction, gap, align, justify, wrap }),
        className,
      )}
      {...props}
    />
  ),
);
Stack.displayName = "Stack";

export { stackVariants };
"#;
