import type { JSONSchema7 } from "json-schema";

export interface NormalizedSchema {
  $schema: string;
  $id: string;
  title: string;
  definitions: Record<string, JSONSchema7>;
}

/**
 * Converts OpenAPI 3.1 schema to JSON Schema draft-07.
 * OpenAPI 3.1 is largely compatible with JSON Schema draft 2020-12,
 * but we want draft-07 for broader tool compatibility.
 */
export function openApiToJsonSchema(schema: Record<string, unknown>): JSONSchema7 {
  const result: Record<string, unknown> = {};

  for (const [key, value] of Object.entries(schema)) {
    // Skip OpenAPI-specific fields
    if (key === "discriminator" || key === "xml" || key === "externalDocs") {
      continue;
    }

    // Handle nullable (OpenAPI 3.0 style)
    if (key === "nullable" && value === true) {
      continue; // Will be handled by type conversion
    }

    // Recursively convert nested schemas
    if (key === "properties" && typeof value === "object" && value !== null) {
      result[key] = {};
      for (const [propName, propSchema] of Object.entries(value as Record<string, unknown>)) {
        (result[key] as Record<string, unknown>)[propName] = openApiToJsonSchema(
          propSchema as Record<string, unknown>
        );
      }
      continue;
    }

    if (key === "items" && typeof value === "object" && value !== null) {
      result[key] = openApiToJsonSchema(value as Record<string, unknown>);
      continue;
    }

    if (key === "additionalProperties" && typeof value === "object" && value !== null) {
      result[key] = openApiToJsonSchema(value as Record<string, unknown>);
      continue;
    }

    if ((key === "oneOf" || key === "anyOf" || key === "allOf") && Array.isArray(value)) {
      result[key] = value.map((item) =>
        typeof item === "object" && item !== null
          ? openApiToJsonSchema(item as Record<string, unknown>)
          : item
      );
      continue;
    }

    // Convert $ref paths from OpenAPI to local definitions
    if (key === "$ref" && typeof value === "string") {
      result[key] = value.replace("#/components/schemas/", "#/definitions/");
      continue;
    }

    result[key] = value;
  }

  // Handle nullable by adding null to type array
  if (schema["nullable"] === true && result["type"]) {
    const currentType = result["type"];
    if (Array.isArray(currentType)) {
      if (!currentType.includes("null")) {
        result["type"] = [...currentType, "null"];
      }
    } else {
      result["type"] = [currentType as string, "null"];
    }
  }

  return result as JSONSchema7;
}

/**
 * Creates a normalized schema with consistent metadata.
 */
export function createNormalizedSchema(
  id: string,
  title: string,
  definitions: Record<string, JSONSchema7>
): NormalizedSchema {
  return {
    $schema: "http://json-schema.org/draft-07/schema#",
    $id: `https://sandbox-daemon/schemas/${id}.json`,
    title,
    definitions,
  };
}

/**
 * Validates a schema against JSON Schema draft-07 meta-schema.
 * Basic validation - checks required fields and structure.
 */
export function validateSchema(schema: unknown): { valid: boolean; errors: string[] } {
  const errors: string[] = [];

  if (typeof schema !== "object" || schema === null) {
    return { valid: false, errors: ["Schema must be an object"] };
  }

  const s = schema as Record<string, unknown>;

  if (s.$schema && typeof s.$schema !== "string") {
    errors.push("$schema must be a string");
  }

  if (s.definitions && typeof s.definitions !== "object") {
    errors.push("definitions must be an object");
  }

  if (s.definitions && typeof s.definitions === "object") {
    for (const [name, def] of Object.entries(s.definitions as Record<string, unknown>)) {
      if (typeof def !== "object" || def === null) {
        errors.push(`Definition "${name}" must be an object`);
      }
    }
  }

  return { valid: errors.length === 0, errors };
}
