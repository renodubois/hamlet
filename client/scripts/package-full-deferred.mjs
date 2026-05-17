#!/usr/bin/env node
import { formatDeferredDistributionMessage } from "./package-config.mjs";

const commandName = process.argv[2] ?? "package:full";
console.error(formatDeferredDistributionMessage(commandName));
process.exitCode = 1;
