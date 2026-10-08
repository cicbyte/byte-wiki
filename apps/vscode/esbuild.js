// esbuild 打包脚本：单文件 bundle src/extension.ts → dist/extension.js
// vscode 模块由 VSCode 运行时提供，标记为 external。
const esbuild = require("esbuild");

const prod = process.argv.includes("--production");
const watch = process.argv.includes("--watch");

/** @type {import("esbuild").BuildOptions} */
const opts = {
  entryPoints: ["src/extension.ts"],
  bundle: true,
  outfile: "dist/extension.js",
  external: ["vscode"],
  format: "cjs",
  platform: "node",
  target: "node16",
  sourcemap: !prod,
  minify: prod,
  logLevel: "info",
};

(async () => {
  try {
    if (watch) {
      const ctx = await esbuild.context(opts);
      await ctx.watch();
      console.log("[esbuild] watching...");
    } else {
      await esbuild.build(opts);
      console.log("[esbuild] build done");
    }
  } catch (e) {
    console.error(e);
    process.exit(1);
  }
})();
