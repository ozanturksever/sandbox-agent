import { fetchWithCache } from "./cache.js";
import { createNormalizedSchema, openApiToJsonSchema, type NormalizedSchema } from "./normalize.js";
import type { JSONSchema7 } from "json-schema";

const OPENAPI_URL =
  "https://raw.githubusercontent.com/sst/opencode/dev/packages/sdk/openapi.json";

// Key schemas we want to extract
const TARGET_SCHEMAS = [
  "Session",
  "Message",
  "Part",
  "Event",
  "PermissionRequest",
  "QuestionRequest",
  "TextPart",
  "ToolCallPart",
  "ToolResultPart",
  "ErrorPart",
];

interface OpenAPISpec {
  components?: {
    schemas?: Record<string, unknown>;
  };
}

export async function extractOpenCodeSchema(): Promise<NormalizedSchema> {
  console.log("Extracting OpenCode schema from OpenAPI spec...");

  const specText = await fetchWithCache(OPENAPI_URL);
  const spec: OpenAPISpec = JSON.parse(specText);

  if (!spec.components?.schemas) {
    throw new Error("OpenAPI spec missing components.schemas");
  }

  const definitions: Record<string, JSONSchema7> = {};

  // Extract all schemas, not just target ones, to preserve references
  for (const [name, schema] of Object.entries(spec.components.schemas)) {
    definitions[name] = openApiToJsonSchema(schema as Record<string, unknown>);
  }

  // Verify target schemas exist
  const missing = TARGET_SCHEMAS.filter((name) => !definitions[name]);
  if (missing.length > 0) {
    console.warn(`  [warn] Missing expected schemas: ${missing.join(", ")}`);
  }

  const found = TARGET_SCHEMAS.filter((name) => definitions[name]);
  console.log(`  [ok] Extracted ${Object.keys(definitions).length} schemas (${found.length} target schemas)`);

  return createNormalizedSchema("opencode", "OpenCode SDK Schema", definitions);
}
