import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import Toast from "./Toast.svelte";

test("dismisses the visible notification through its accessible control", async () => {
  const user = userEvent.setup();
  let dismissedId: number | null = null;
  render(Toast, {
    toasts: [{ id: 7, message: "Opened 2 diffs", timeoutMs: 4000 }],
    ondismiss: (id) => {
      dismissedId = id;
    },
  });

  expect(screen.getByRole("status")).toHaveTextContent("Opened 2 diffs");
  await user.click(screen.getByRole("button", { name: "Dismiss" }));
  expect(dismissedId).toBe(7);
});
