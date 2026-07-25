import { bench } from "vitest";
import { computeWheelScroll } from "./wheel";

// Models a horizontal scroll burst over wide diff panes: wheel events fire per
// frame, so the decision must stay allocation-free.
const SCROLLERS = [
  { scrollWidth: 4000, clientWidth: 900, scrollLeft: 0 },
  { scrollWidth: 4000, clientWidth: 900, scrollLeft: 1500 },
  { scrollWidth: 900, clientWidth: 900, scrollLeft: 0 },
  { scrollWidth: 4000, clientWidth: 900, scrollLeft: 3100 },
];
const EVENTS = [
  { deltaX: 0, deltaY: 40, ctrlKey: false },
  { deltaX: 25, deltaY: 3, ctrlKey: false },
  { deltaX: 0, deltaY: -120, ctrlKey: false },
  { deltaX: 0, deltaY: 40, ctrlKey: true },
];

bench("computeWheelScroll over a scroll burst", () => {
  for (const scroller of SCROLLERS) {
    for (const event of EVENTS) computeWheelScroll(scroller, event);
  }
});
