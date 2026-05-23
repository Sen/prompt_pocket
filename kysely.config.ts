import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "kysely-ctl";
import { createPromptPocketDatabase } from "./src/database";

const configDir = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  kysely: createPromptPocketDatabase(),
  migrations: {
    migrationFolder: `${configDir}/migrations`
  }
});
