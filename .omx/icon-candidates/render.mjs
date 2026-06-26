import { Resvg } from "@resvg/resvg-js";
import { readFileSync, writeFileSync } from "node:fs";

const files = [
  "spiral-A-helix",
  "spiral-B-archimedean",
  "spiral-C-tristrand-warm",
];

for (const name of files) {
  const svg = readFileSync(`.omx/icon-candidates/${name}.svg`, "utf-8");
  const resvg = new Resvg(svg, { fitTo: { mode: "width", value: 512 } });
  const png = resvg.render().asPng();
  writeFileSync(`.omx/icon-candidates/${name}.png`, png);
  console.log(`wrote ${name}.png`);
}
