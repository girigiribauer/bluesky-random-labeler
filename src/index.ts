import dotenv from "dotenv";
import { LabelerServer } from "@skyware/labeler";
import { createHash } from "node:crypto";
import { serve } from "@hono/node-server";
import { Hono } from "hono";

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
    labels,
  });
});

// Provide a health check or root
app.get("/", (c) => c.text("Omikuji Labeler is running (HTTP Only)."));

const port = 4000;
console.log(`Labeler server running on port ${port}`);

// Explicitly NOT adding WebSocket support to force AppView fallback to polling
serve({
  fetch: app.fetch,
  port,
  hostname: "0.0.0.0"
});
