// OpenAPI $ref resolution and schema utilities

/**
 * Resolve a $ref string like "#/components/schemas/Pet" to the actual object in the spec.
 */
export function resolveRef(ref: string, spec: any): any {
  if (!ref.startsWith('#/')) return undefined;
  const parts = ref.slice(2).split('/');
  let current = spec;
  for (const part of parts) {
    current = current?.[part];
    if (current === undefined) return undefined;
  }
  return current;
}

/**
 * Recursively resolve a schema, following $ref, allOf/oneOf/anyOf, properties, items.
 * Max depth protects against circular references.
 */
export function resolveSchema(schema: any, spec: any, depth = 0): any {
  if (!schema || depth > 10) return schema;

  if (schema.$ref) {
    const resolved = resolveRef(schema.$ref, spec);
    if (!resolved) return { type: 'object', _refName: schema.$ref.split('/').pop() };
    return resolveSchema(resolved, spec, depth + 1);
  }

  if (schema.allOf) {
    const merged: any = { type: 'object', properties: {}, required: [] };
    for (const sub of schema.allOf) {
      const resolved = resolveSchema(sub, spec, depth + 1);
      if (resolved?.properties) Object.assign(merged.properties, resolved.properties);
      if (resolved?.required) merged.required.push(...resolved.required);
      if (resolved?.description && !merged.description) merged.description = resolved.description;
    }
    return merged;
  }

  if (schema.oneOf || schema.anyOf) {
    const variants = (schema.oneOf || schema.anyOf).map((s: any) => resolveSchema(s, spec, depth + 1));
    return { ...schema, _variants: variants, type: schema.type || 'oneOf' };
  }

  const result = { ...schema };

  if (result.properties) {
    const resolved: Record<string, any> = {};
    for (const [key, val] of Object.entries(result.properties)) {
      resolved[key] = resolveSchema(val, spec, depth + 1);
    }
    result.properties = resolved;
  }

  if (result.items) {
    result.items = resolveSchema(result.items, spec, depth + 1);
  }

  if (result.additionalProperties && typeof result.additionalProperties === 'object') {
    result.additionalProperties = resolveSchema(result.additionalProperties, spec, depth + 1);
  }

  return result;
}

/**
 * Extract the request body schema from an operation.
 * Supports OpenAPI 3.x (requestBody.content) and Swagger 2.0 (parameters with in:"body").
 */
export function extractRequestBody(operation: any, spec: any): any | null {
  // OpenAPI 3.x
  const content = operation?.requestBody?.content;
  if (content) {
    const jsonSchema = content['application/json']?.schema;
    if (jsonSchema) return resolveSchema(jsonSchema, spec);
  }

  // Swagger 2.0: look for a parameter with in:"body"
  const params = operation?.parameters;
  if (Array.isArray(params)) {
    const bodyParam = params.find((p: any) => p.in === 'body' && p.schema);
    if (bodyParam) return resolveSchema(bodyParam.schema, spec);
  }

  return null;
}

/**
 * Extract response schemas keyed by status code.
 * Supports OpenAPI 3.x (content.application/json.schema) and Swagger 2.0 (schema directly on response).
 */
export function extractResponseSchemas(operation: any, spec: any): Record<string, any> {
  const result: Record<string, any> = {};
  if (!operation?.responses) return result;

  for (const [status, resp] of Object.entries(operation.responses as Record<string, any>)) {
    // OpenAPI 3.x
    const content = resp?.content;
    if (content) {
      const jsonSchema = content['application/json']?.schema;
      if (jsonSchema) {
        result[status] = resolveSchema(jsonSchema, spec);
        continue;
      }
    }
    // Swagger 2.0: schema directly on response object
    if (resp?.schema) {
      result[status] = resolveSchema(resp.schema, spec);
    }
  }
  return result;
}

/**
 * Generate an example value from a resolved schema.
 */
export function generateExample(schema: any, depth = 0): any {
  if (!schema || depth > 5) return null;

  if (schema.example !== undefined) return schema.example;
  if (schema.default !== undefined) return schema.default;

  if (schema._variants) {
    return generateExample(schema._variants[0], depth + 1);
  }

  const type = schema.type;

  if (type === 'object' || schema.properties) {
    const obj: Record<string, any> = {};
    if (schema.properties) {
      for (const [key, prop] of Object.entries(schema.properties as Record<string, any>)) {
        obj[key] = generateExample(prop, depth + 1);
      }
    }
    return obj;
  }

  if (type === 'array') {
    const item = generateExample(schema.items, depth + 1);
    return item !== null ? [item] : [];
  }

  if (schema.enum?.length) return schema.enum[0];

  switch (type) {
    case 'string':
      if (schema.format === 'date-time') return '2026-01-01T00:00:00Z';
      if (schema.format === 'date') return '2026-01-01';
      if (schema.format === 'email') return 'user@example.com';
      if (schema.format === 'uri' || schema.format === 'url') return 'https://example.com';
      if (schema.format === 'uuid') return '550e8400-e29b-41d4-a716-446655440000';
      return 'string';
    case 'integer':
      return schema.minimum ?? 0;
    case 'number':
      return schema.minimum ?? 0.0;
    case 'boolean':
      return false;
    default:
      return null;
  }
}
