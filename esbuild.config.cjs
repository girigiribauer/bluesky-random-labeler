const esbuild = require("esbuild");

esbuild
  .build({
    entryPoints: ["src/index.ts"],
    bundle: true,
    packages: "external",
    platform: "node",
    target: "node22",
    format: "cjs",
    outfile: "dist/index.cjs",
  })
  .catch(() => process.exit(1));
