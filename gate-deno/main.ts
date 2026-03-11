interface ServiceEntry {
  name: string;
  ip: string;
  port: number;
  "api-key"?: string;
  tls?: boolean;
  timeout?: number;
  host?: string;
}

interface Service extends ServiceEntry {
  baseUrl: string;
  timeoutMs: number;
}

const services = new Map<string, Service>();

const SKIP_REQUEST_HEADERS = new Set([
  "host", "accept-encoding", "connection", "transfer-encoding", "x-api-key",
]);
const SKIP_RESPONSE_HEADERS = new Set([
  "transfer-encoding", "content-length",
]);

// Load PROXY config
const proxyJson = Deno.env.get("PROXY") ?? "[]";
try {
  const entries: ServiceEntry[] = JSON.parse(proxyJson);
  for (const entry of entries) {
    const scheme = entry.tls ? "https" : "http";
    services.set(entry.name, {
      ...entry,
      baseUrl: `${scheme}://${entry.ip}:${entry.port}`,
      timeoutMs: (entry.timeout && entry.timeout > 0 ? entry.timeout : 30) * 1000,
    });
    console.log(
      `Service registered: ${entry.name} -> ${entry.ip}:${entry.port} (api-key: ${entry["api-key"] ? "***" : "none"})`,
    );
  }
  console.log(`Loaded ${services.size} services from PROXY`);
} catch (e) {
  throw new Error(`Failed to parse PROXY: ${e}`);
}

function constantTimeEq(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let result = 0;
  for (let i = 0; i < a.length; i++) {
    result |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  return result === 0;
}

async function handler(req: Request): Promise<Response> {
  const start = performance.now();
  const url = new URL(req.url);
  const method = req.method;
  const path = url.pathname;

  // Health
  if (path === "/health") {
    return Response.json({ status: "ok", version: "1.0.0" });
  }

  // Services list
  if (path === "/services" && method === "GET") {
    const list = [...services.values()].map((s) => ({
      name: s.name,
      url: s.baseUrl,
      timeout: s.timeoutMs / 1000,
      auth: !!s["api-key"],
      ...(s.host ? { host: s.host } : {}),
    }));
    return Response.json({ services: list, total: list.length });
  }

  // Parse /{service_name}/{remaining}
  const trimmed = path.replace(/^\//, "");
  const slashIdx = trimmed.indexOf("/");
  const serviceName = slashIdx > 0 ? trimmed.substring(0, slashIdx) : trimmed;
  const remaining = slashIdx > 0 ? trimmed.substring(slashIdx) : "/";

  const service = services.get(serviceName);
  if (!service) {
    console.warn(`${method} ${path} -> 404 (unknown service: ${serviceName})`);
    return Response.json({ error: `Service not found: ${serviceName}` }, { status: 404 });
  }

  // API key validation
  const apiKey = service["api-key"];
  if (apiKey) {
    const clientKey = req.headers.get("X-API-KEY") ?? "";
    if (!constantTimeEq(clientKey, apiKey)) {
      console.warn(`${method} ${path} -> 401 (invalid or missing X-API-KEY for service: ${serviceName})`);
      return Response.json({ error: "Unauthorized: invalid or missing X-API-KEY" }, { status: 401 });
    }
  }

  const query = url.search;
  const targetUrl = `${service.baseUrl}${remaining}${query}`;

  // Build upstream headers
  const headers = new Headers();
  for (const [name, value] of req.headers) {
    if (!SKIP_REQUEST_HEADERS.has(name.toLowerCase())) {
      headers.set(name, value);
    }
  }
  if (service.host) {
    headers.set("host", service.host);
  }

  // Forward request
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), service.timeoutMs);

  try {
    const body = req.body && !["GET", "HEAD"].includes(method) ? await req.arrayBuffer() : null;

    const resp = await fetch(targetUrl, {
      method,
      headers,
      body,
      signal: controller.signal,
      redirect: "follow",
    });

    clearTimeout(timer);
    const ms = (performance.now() - start).toFixed(1);
    console.log(`${method} ${path} -> ${resp.status} ${ms}ms [${targetUrl}]`);

    // Build response headers
    const respHeaders = new Headers();
    for (const [name, value] of resp.headers) {
      if (!SKIP_RESPONSE_HEADERS.has(name.toLowerCase())) {
        respHeaders.set(name, value);
      }
    }

    return new Response(resp.body, {
      status: resp.status,
      headers: respHeaders,
    });
  } catch (e) {
    clearTimeout(timer);
    const ms = (performance.now() - start).toFixed(1);

    if (e instanceof DOMException && e.name === "AbortError") {
      console.error(`${method} ${path} -> 504 ${ms}ms [${targetUrl}] timeout after ${service.timeoutMs / 1000}s`);
      return Response.json(
        { error: `Request timed out after ${service.timeoutMs / 1000}s` },
        { status: 504 },
      );
    }

    console.error(`${method} ${path} -> 502 ${ms}ms [${targetUrl}] error: ${e}`);
    return Response.json(
      { error: `Upstream unreachable: ${serviceName}` },
      { status: 502 },
    );
  }
}

const port = parseInt(Deno.env.get("PORT") ?? "8080");
console.log(`gate-deno listening on :${port}`);
Deno.serve({ port }, handler);
