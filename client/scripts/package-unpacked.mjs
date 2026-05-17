#!/usr/bin/env node
import { packager } from "@electron/packager";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  ALPHA_PACKAGE_METADATA,
  assertPackagePrerequisites,
  createPackagerOptions,
  packageTargetFromEnvironment,
  readPackageVersion,
  writeLastUnpackedManifest,
} from "./package-config.mjs";

const rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

try {
  await assertPackagePrerequisites(rootDir);
  const appVersion = await readPackageVersion(rootDir);
  const target = packageTargetFromEnvironment();
  const options = createPackagerOptions({ rootDir, appVersion, ...target });
  const packagePaths = await packager(options);

  if (packagePaths.length !== 1) {
    throw new Error(
      `Expected one unpacked package for ${target.platform}-${target.arch}, got ${packagePaths.length}.`,
    );
  }

  const [packagePath] = packagePaths;
  await writeLastUnpackedManifest({ rootDir, packagePath, appVersion, ...target });

  console.log(`${ALPHA_PACKAGE_METADATA.productName} unpacked package created:`);
  console.log(`  ${packagePath}`);
  console.log("Launch it with: npm run package:launch");
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
}
