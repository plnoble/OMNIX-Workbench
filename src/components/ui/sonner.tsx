/**
 * Sonner Toast Component — lightweight toast notifications
 *
 * Replaces all alert() calls with non-blocking toasts.
 * Usage: import { toast } from "sonner"; toast.success("Saved!")
 */

import { Toaster as Sonner, toast } from "sonner"

export { toast }

export function Toaster() {
  return (
    <Sonner
      className="toaster-group"
      toastOptions={{
        classNames: {
          toast:
            "group toast group-[.toaster]:bg-[rgba(15,15,20,0.9)] group-[.toaster]:text-foreground group-[.toaster]:border-border group-[.toaster]:backdrop-blur-xl",
          description: "group-[.toast]:text-muted-foreground",
          actionButton:
            "group-[.toast]:bg-accent group-[.toast]:text-accent-foreground",
          cancelButton:
            "group-[.toast]:bg-muted group-[.toast]:text-muted-foreground",
        },
      }}
    />
  )
}
