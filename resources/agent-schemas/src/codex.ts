import { createGenerator, type Config } from "ts-json-schema-generator";
import { existsSync } from "fs";
import { join } from "path";
import { createNormalizedSchema, type NormalizedSchema } from "./normalize.js";
import type { JSONSchema7 } from "json-schema";

// Try multiple possible paths for the SDK types
const POSSIBLE_PATHS = [
  "node_modules/@openai/codex/dist/index.d.ts",
  "node_modules/@openai/codex/dist/types.d.ts",
  "node_modules/@openai/codex/index.d.ts",
];

// Key types we want to extract
const TARGET_TYPES = [
  "ThreadEvent",
  "ThreadItem",
  "CodexOptions",
  "ThreadOptions",
  "Input",
  "ResponseItem",
  "FunctionCall",
  "Message",
];

function findTypesPath(): string | null {
  const baseDir = join(import.meta.dirname, "..");

  for (const relativePath of POSSIBLE_PATHS) {
    const fullPath = join(baseDir, relativePath);
    if (existsSync(fullPath)) {
      return fullPath;
    }
  }

  return null;
}

export async function extractCodexSchema(): Promise<NormalizedSchema> {
  console.log("Extracting Codex SDK schema...");

  const typesPath = findTypesPath();

  if (!typesPath) {
    console.log("  [warn] Codex SDK types not found, using fallback schema");
    return createFallbackSchema();
  }

  console.log(`  [found] ${typesPath}`);

  const config: Config = {
    path: typesPath,
    tsconfig: join(import.meta.dirname, "..", "tsconfig.json"),
    type: "*",
    skipTypeCheck: true,
    topRef: false,
    expose: "export",
    jsDoc: "extended",
  };

  try {
    const generator = createGenerator(config);
    const schema = generator.createSchema(config.type);

    const definitions: Record<string, JSONSchema7> = {};

    if (schema.definitions) {
      for (const [name, def] of Object.entries(schema.definitions)) {
        definitions[name] = def as JSONSchema7;
      }
    }

    // Verify target types exist
    const found = TARGET_TYPES.filter((name) => definitions[name]);
    const missing = TARGET_TYPES.filter((name) => !definitions[name]);

    if (missing.length > 0) {
      console.log(`  [warn] Missing expected types: ${missing.join(", ")}`);
    }

    console.log(`  [ok] Extracted ${Object.keys(definitions).length} types (${found.length} target types)`);

    return createNormalizedSchema("codex", "Codex SDK Schema", definitions);
  } catch (error) {
    console.log(`  [error] Schema generation failed: ${error}`);
    console.log("  [fallback] Using embedded schema definitions");
    return createFallbackSchema();
  }
}

function createFallbackSchema(): NormalizedSchema {
  // Fallback schema based on known SDK structure
  const definitions: Record<string, JSONSchema7> = {
    ThreadEvent: {
      type: "object",
      properties: {
        type: {
          type: "string",
          enum: ["thread.created", "thread.updated", "item.created", "item.updated", "error"],
        },
        thread_id: { type: "string" },
        item: { $ref: "#/definitions/ThreadItem" },
        error: { type: "object" },
      },
      required: ["type"],
    },
    ThreadItem: {
      type: "object",
      properties: {
        id: { type: "string" },
        type: { type: "string", enum: ["message", "function_call", "function_result"] },
        role: { type: "string", enum: ["user", "assistant", "system"] },
        content: {
          oneOf: [{ type: "string" }, { type: "array", items: { type: "object" } }],
        },
        status: { type: "string", enum: ["pending", "in_progress", "completed", "failed"] },
      },
      required: ["id", "type"],
    },
    CodexOptions: {
      type: "object",
      properties: {
        apiKey: { type: "string" },
        model: { type: "string" },
        baseURL: { type: "string" },
        maxTokens: { type: "number" },
        temperature: { type: "number" },
      },
    },
    ThreadOptions: {
      type: "object",
      properties: {
        instructions: { type: "string" },
        tools: { type: "array", items: { type: "object" } },
        model: { type: "string" },
        workingDirectory: { type: "string" },
      },
    },
    Input: {
      type: "object",
      properties: {
        type: { type: "string", enum: ["text", "file", "image"] },
        content: { type: "string" },
        path: { type: "string" },
        mimeType: { type: "string" },
      },
      required: ["type"],
    },
    ResponseItem: {
      type: "object",
      properties: {
        type: { type: "string" },
        id: { type: "string" },
        content: { type: "string" },
        function_call: { $ref: "#/definitions/FunctionCall" },
      },
    },
    FunctionCall: {
      type: "object",
      properties: {
        name: { type: "string" },
        arguments: { type: "string" },
        call_id: { type: "string" },
      },
      required: ["name", "arguments"],
    },
    Message: {
      type: "object",
      properties: {
        role: { type: "string", enum: ["user", "assistant", "system"] },
        content: { type: "string" },
      },
      required: ["role", "content"],
    },
  };

  console.log(`  [ok] Using fallback schema with ${Object.keys(definitions).length} definitions`);

  return createNormalizedSchema("codex", "Codex SDK Schema", definitions);
}
