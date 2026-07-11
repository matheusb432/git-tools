import fsd from "@feature-sliced/steiger-plugin";
import { defineConfig } from "steiger";

export default defineConfig([
  ...fsd.configs.recommended,
  {
    files: ["./frontend/viewer/pages/history/.gitkeep"],
    rules: {
      "fsd/no-segmentless-slices": "off",
    },
  },
]);
