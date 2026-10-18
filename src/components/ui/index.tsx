// Small, accessible UI primitives (Radix for dialogs/menus, Tailwind for styling).

import * as RadixDialog from "@radix-ui/react-dialog";
import clsx from "clsx";
import { X } from "lucide-react";
import { forwardRef, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode, type SelectHTMLAttributes } from "react";

type Variant = "primary" | "secondary" | "ghost" | "danger";

export const Button = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; size?: "sm" | "md" }>(
  function Button({ variant = "secondary", size = "md", className, ...props }, ref) {
    return (
      <button
        ref={ref}
        className={clsx(
          "inline-flex items-center justify-center gap-1.5 rounded-md font-medium transition-colors",
          "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
          "disabled:pointer-events-none disabled:opacity-45",
          size === "sm" ? "h-7 px-2.5 text-xs" : "h-8 px-3 text-[13px]",
          variant === "primary" && "bg-accent text-accent-fg hover:brightness-110",
          variant === "secondary" && "border border-line bg-panel text-fg hover:bg-panel-2",
          variant === "ghost" && "text-fg hover:bg-panel-2",
          variant === "danger" && "bg-err text-white hover:brightness-110",
          className,
        )}
        {...props}
      />
    );
  },
);

export const IconButton = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement> & { label: string }>(
  function IconButton({ label, className, children, ...props }, ref) {
    return (
      <button
        ref={ref}
        aria-label={label}
        title={label}
        className={clsx(
          "inline-flex h-8 min-w-8 items-center justify-center gap-1.5 rounded-md px-2 text-fg transition-colors",
          "hover:bg-panel-2 focus-visible:outline-2 focus-visible:outline-accent disabled:opacity-40 disabled:pointer-events-none",
          className,
        )}
        {...props}
      >
        {children}
      </button>
    );
  },
);

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(function Input({ className, ...props }, ref) {
  return (
    <input
      ref={ref}
      className={clsx(
        "h-8 w-full rounded-md border border-line bg-panel px-2.5 text-[13px] text-fg placeholder:text-muted",
        "focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/25 disabled:opacity-60",
        className,
      )}
      {...props}
    />
  );
});

export function Select({ className, ...props }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select
      className={clsx(
        "h-8 rounded-md border border-line bg-panel px-2 text-[13px] text-fg focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/25",
        className,
      )}
      {...props}
    />
  );
}

export function Switch({ checked, onChange, label, description, disabled }: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  description?: string;
  disabled?: boolean;
}) {
  return (
    <label className={clsx("flex cursor-pointer items-start justify-between gap-4 py-1.5", disabled && "opacity-50")}>
      <span>
        <span className="block text-[13px] text-fg">{label}</span>
        {description && <span className="block text-xs text-muted">{description}</span>}
      </span>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={clsx(
          "relative mt-0.5 h-5 w-9 shrink-0 rounded-full transition-colors focus-visible:outline-2 focus-visible:outline-accent",
          checked ? "bg-accent" : "bg-line",
        )}
      >
        <span className={clsx("absolute top-0.5 left-0.5 h-4 w-4 rounded-full bg-white shadow transition-transform", checked && "translate-x-4")} />
      </button>
    </label>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="block space-y-1">
      <span className="block text-xs font-medium text-muted">{label}</span>
      {children}
      {hint && <span className="block text-xs text-muted">{hint}</span>}
    </label>
  );
}

export function ProgressBar({ value, active, error }: { value: number | null; active?: boolean; error?: boolean }) {
  const indeterminate = value === null;
  return (
    <div
      className="h-1.5 w-full overflow-hidden rounded-full bg-panel-2"
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={value === null ? undefined : Math.round(value)}
    >
      <div
        className={clsx(
          "h-full rounded-full transition-[width] duration-500 ease-out",
          error ? "bg-err" : value !== null && value >= 100 ? "bg-ok" : "bg-accent",
          active && "progress-stripes",
          indeterminate && active && "w-1/3",
        )}
        style={indeterminate ? undefined : { width: `${value}%` }}
      />
    </div>
  );
}

export function Dialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  footer,
  width = "max-w-lg",
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  children: ReactNode;
  footer?: ReactNode;
  width?: string;
}) {
  return (
    <RadixDialog.Root open={open} onOpenChange={onOpenChange}>
      <RadixDialog.Portal>
        <RadixDialog.Overlay className="fixed inset-0 z-40 bg-black/35 animate-fade-in" />
        <RadixDialog.Content
          className={clsx(
            "fixed top-1/2 left-1/2 z-50 flex max-h-[88vh] w-[calc(100vw-32px)] -translate-x-1/2 -translate-y-1/2 flex-col",
            "rounded-xl border border-line bg-panel shadow-2xl animate-pop-in focus:outline-none",
            width,
          )}
        >
          <div className="flex items-start justify-between gap-4 border-b border-line px-5 py-3.5">
            <div>
              <RadixDialog.Title className="text-[15px] font-semibold text-fg">{title}</RadixDialog.Title>
              {description ? (
                <RadixDialog.Description className="mt-0.5 text-xs text-muted">{description}</RadixDialog.Description>
              ) : (
                <RadixDialog.Description className="sr-only">{title}</RadixDialog.Description>
              )}
            </div>
            <RadixDialog.Close asChild>
              <IconButton label="Schließen" className="-mr-2">
                <X size={16} />
              </IconButton>
            </RadixDialog.Close>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">{children}</div>
          {footer && <div className="flex flex-wrap items-center justify-end gap-2 border-t border-line px-5 py-3">{footer}</div>}
        </RadixDialog.Content>
      </RadixDialog.Portal>
    </RadixDialog.Root>
  );
}
