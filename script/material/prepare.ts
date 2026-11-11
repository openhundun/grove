import { existsSync } from "node:fs";
import { join } from "node:path";

const packageDir = join(import.meta.dir, "node_modules/material-icon-theme");
const declaredPackage = await Bun.file(join(import.meta.dir, "package.json")).json();
const declaredVersion = declaredPackage.dependencies["material-icon-theme"];
const installedPackage = Bun.file(join(packageDir, "package.json"));
const installedVersion = (await installedPackage.exists()) ? (await installedPackage.json()).version : null;

if (installedVersion !== declaredVersion || !existsSync(join(packageDir, "icons")) || !existsSync(join(packageDir, "dist/material-icons.json"))) {
    const result = Bun.spawnSync(["bun", "install", "--production", "--ignore-scripts", "--no-save", "--registry", "https://registry.npmmirror.com"], {
        cwd: import.meta.dir,
        stderr: "inherit",
        stdout: "inherit",
    });
    if (result.exitCode !== 0) process.exit(result.exitCode);
}
