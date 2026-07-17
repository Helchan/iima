import { createServer } from "node:http";
import { createReadStream, statSync } from "node:fs";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";
import { dirname } from "node:path";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const base = join(root, "src");
const port = Number(process.env.PORT || 1420);

const contentTypes = {
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml",
};

createServer((req, res) => {
  const urlPath = decodeURIComponent(new URL(req.url ?? "/", `http://127.0.0.1:${port}`).pathname);
  const requested = normalize(join(base, urlPath === "/" ? "index.html" : urlPath));
  if (!requested.startsWith(base)) {
    res.writeHead(403);
    res.end("Forbidden");
    return;
  }

  try {
    const stat = statSync(requested);
    if (!stat.isFile()) throw new Error("not a file");
    res.writeHead(200, { "Content-Type": contentTypes[extname(requested)] ?? "application/octet-stream" });
    createReadStream(requested).pipe(res);
  } catch {
    res.writeHead(404);
    res.end("Not found");
  }
}).listen(port, "127.0.0.1", () => {
  console.log(`IIMA frontend dev server: http://127.0.0.1:${port}`);
});
