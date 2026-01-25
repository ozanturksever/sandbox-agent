import { createGenerator, type Config } from "ts-json-schema-generator";
import { existsSync, readFileSync } from "fs";
import { join, dirname } from "path";
import { createNormalizedSchema, type NormalizedSchema } from "./normalize.js";
import type { JSONSchema7 } from "json-schema";

// Try multiple possible paths for the SDK types
const POSSIBLE_PATHS = [
  "node_modules/@anthropic-ai/claude-code/sdk-tools.d.ts",
  "node_modules/@anthropic-ai/claude-code/dist/index.d.ts",
  "node_modules/@anthropic-ai/claude-code/dist/types.d.ts",
  "node_modules/@anthropic-ai/claude-code/index.d.ts",
];

// Key types we want to extract
const TARGET_TYPES = [
  "ToolInputSchemas",
  "AgentInput",
  "BashInput",
  "FileEditInput",
  "FileReadInput",
  "FileWriteInput",
  "GlobInput",
  "GrepInput",
  "WebFetchInput",
  "WebSearchInput",
  "AskUserQuestionInput",
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

export async function extractClaudeSchema(): Promise<NormalizedSchema> {
  console.log("Extracting Claude Code SDK schema...");

  const typesPath = findTypesPath();

  if (!typesPath) {
    console.log("  [warn] Claude Code SDK types not found, using fallback schema");
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

    return createNormalizedSchema("claude", "Claude Code SDK Schema", definitions);
  } catch (error) {
    console.log(`  [error] Schema generation failed: ${error}`);
    console.log("  [fallback] Using embedded schema definitions");
    return createFallbackSchema();
  }
}

function createFallbackSchema(): NormalizedSchema {
  // Fallback schema based on known SDK structure
  const definitions: Record<string, JSONSchema7> = {
    SDKMessage: {
      type: "object",
      properties: {
        type: { type: "string", enum: ["user", "assistant", "result"] },
        content: { type: "string" },
        timestamp: { type: "string", format: "date-time" },
      },
      required: ["type"],
    },
    SDKResultMessage: {
      type: "object",
      properties: {
        type: { type: "string", const: "result" },
        result: { type: "object" },
        error: { type: "string" },
        duration_ms: { type: "number" },
      },
      required: ["type"],
    },
    Options: {
      type: "object",
      properties: {
        model: { type: "string" },
        maxTokens: { type: "number" },
        temperature: { type: "number" },
        systemPrompt: { type: "string" },
        tools: { type: "array", items: { type: "string" } },
        allowedTools: { type: "array", items: { type: "string" } },
        workingDirectory: { type: "string" },
      },
    },
    BashInput: {
      type: "object",
      properties: {
        command: { type: "string" },
        timeout: { type: "number" },
        workingDirectory: { type: "string" },
      },
      required: ["command"],
    },
    FileEditInput: {
      type: "object",
      properties: {
        path: { type: "string" },
        oldText: { type: "string" },
        newText: { type: "string" },
      },
      required: ["path", "oldText", "newText"],
    },
    FileReadInput: {
      type: "object",
      properties: {
        path: { type: "string" },
        startLine: { type: "number" },
        endLine: { type: "number" },
      },
      required: ["path"],
    },
    FileWriteInput: {
      type: "object",
      properties: {
        path: { type: "string" },
        content: { type: "string" },
      },
      required: ["path", "content"],
    },
    GlobInput: {
      type: "object",
      properties: {
        pattern: { type: "string" },
        path: { type: "string" },
      },
      required: ["pattern"],
    },
    GrepInput: {
      type: "object",
      properties: {
        pattern: { type: "string" },
        path: { type: "string" },
        include: { type: "string" },
      },
      required: ["pattern"],
    },
  };

  console.log(`  [ok] Using fallback schema with ${Object.keys(definitions).length} definitions`);

  return createNormalizedSchema("claude", "Claude Code SDK Schema", definitions);
}
