import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";
export type { WithElementRef, WithoutChild, WithoutChildren, WithoutChildrenOrChild } from "bits-ui";

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
