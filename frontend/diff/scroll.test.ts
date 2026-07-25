import { expect, test } from "vitest";
import { scrollLandOn } from "./scroll";

test("scrollLandOn converges target top to stickyTop despite estimate drift", () => {
  let scrollTop = 0;
  let realized = 0; // grows each frame: intervening content gets taller as it realizes
  const target = {
    getBoundingClientRect: () => ({ top: 800 - scrollTop + realized * 40 }),
  };
  const scroller = {
    get scrollTop() {
      return scrollTop;
    },
    set scrollTop(v: number) {
      scrollTop = v;
    },
    getBoundingClientRect: () => ({ top: 0 }),
  };
  const queue: FrameRequestCallback[] = [];
  const raf = (cb: FrameRequestCallback) => {
    queue.push(cb);
    return queue.length;
  };
  scrollLandOn(target, scroller, { stickyTop: 48, raf, maxFrames: 20 });
  // drain frames; each frame realizes a bit more layout
  for (let i = 0; i < 30 && queue.length; i++) {
    realized = Math.min(realized + 1, 3);
    queue.shift()!(0);
  }
  expect(Math.abs(target.getBoundingClientRect().top - 48)).toBeLessThanOrEqual(1);
});
