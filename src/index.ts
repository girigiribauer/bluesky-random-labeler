import dotenv from "dotenv";
import { LabelerServer } from "@skyware/labeler";
import { createHash } from "node:crypto";
import { serve } from "@hono/node-server";
import { Hono } from "hono";
import { WebSocketServer } from "ws";

const FORTUNES = [
  { val: "daikichi", threshold: 6 },   // 6%
  { val: "kichi", threshold: 28 },  // 22% (+6)
  { val: "chukichi", threshold: 50 },  // 22% (+28)
  { val: "shokichi", threshold: 70 },  // 20% (+50)
  { val: "suekichi", threshold: 88 },  // 18% (+70)
  { val: "kyo", threshold: 97 },  // 9%  (+88)
  { val: "daikyo", threshold: 100 }, // 3%  (+97)
];

function getDailyFortune(did: string): string {
  const jstNow = new Date(new Date().toLocaleString("en-US", { timeZone: "Asia/Tokyo" }));
  const dateStr = jstNow.toISOString().split("T")[0];

  const seed = did + dateStr;

  const hash = createHash("sha256").update(seed).digest();

  const val = hash.readUInt32BE(0) % 100;

  for (const fortune of FORTUNES) {
    if (val < fortune.threshold) {
      return fortune.val;
    }
  }
  return "kichi";
}

dotenv.config();

const app = new Hono();

const labeler = new LabelerServer({
  did: process.env.LABELER_DID ?? "",
  signingKey: process.env.SIGNING_KEY ?? "",
  dbPath: ":memory:",
});

// HTTP: queryLabels
app.get("/xrpc/com.atproto.label.queryLabels", async (c) => {
  console.log("Received queryLabels");

  const labels = [];

  // Hono handles query params slightly differently, but ?uriPatterns=a&uriPatterns=b works naturally if we access it right.
  // c.req.queries('uriPatterns') returns string[] | undefined
  const patterns = c.req.queries('uriPatterns');

  if (patterns && patterns.length > 0) {
    console.log("Received queryLabels request:", patterns);

    for (const uri of patterns) {
      try {
        const fortune = getDailyFortune(uri);
        console.log(`Providing fortune for ${uri}: ${fortune}`);

        const label = await labeler.createLabel({
          uri,
          val: fortune,
        });
        labels.push(label);
      } catch (e) {
        console.error(`Failed to create label for ${uri}`, e);
      }
    }
  }

  return c.json({
    cursor: Date.now().toString(),
    labels,
  });
});

// Provide a health check or root
app.get("/", (c) => c.text("Omikuji Labeler is running."));

const port = 4000;
console.log(`Labeler server running on port ${port}`);

const server = serve({
  fetch: app.fetch,
  port,
  hostname: "0.0.0.0"
});

// WebSocket: subscribeLabels (Dummy Firehose)
const wss = new WebSocketServer({ server: server as any, path: "/xrpc/com.atproto.label.subscribeLabels" });

wss.on("connection", (ws) => {
  console.log("New WebSocket subscription connected!");

  // Optional: Send an empty cursor or header frame if needed,
  // but keeping it open is usually enough for the handshake.
  // We can send a ping periodically to keep it alive.
  const interval = setInterval(() => {
    if (ws.readyState === ws.OPEN) {
      ws.ping();
    }
  }, 30000);

  ws.on("close", () => {
    clearInterval(interval);
    console.log("WebSocket subscription closed");
  });
});
