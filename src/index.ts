import { serve } from "@hono/node-server";
import { Hono } from "hono";
import { LabelerServer } from "@skyware/labeler";
import { Bot } from "@skyware/bot";
import dotenv from "dotenv";
import { processUser } from "./labeling.js";
import { startMidnightScheduler } from "./scheduler.js";
import { initDB } from "./db.js";

dotenv.config();

// Initialize DB Schema
initDB();

const app = new Hono();
const port = Number(process.env.PORT) || 3000;

// Initialize Labeler (Stateful with SQLite)
const labeler = new LabelerServer({
  did: process.env.LABELER_DID ?? "",
  signingKey: process.env.SIGNING_KEY ?? "",
  dbPath: process.env.DB_PATH ?? "data/labels.db",
});

const bot = new Bot();

app.get("/", (c) => {
  return c.text("Omikuji Labeler is running!");
});

serve({
  fetch: app.fetch,
  port,
  hostname: "0.0.0.0"
}, (info) => {
  console.log(`Listening on ${info.port}`);

  labeler.start({ port: 0 }, (error, address) => {
    if (error) {
      console.error("Labeler failed to start:", error);
    } else {
      console.log("Labeler internal server started.");
    }
  });
});

async function main() {
  try {
    await bot.login({
      identifier: process.env.LABELER_DID ?? "",
      password: process.env.LABELER_PASSWORD ?? "",
    });
    console.log("Bot logged in!");

    startMidnightScheduler(bot, labeler);

    bot.on("follow", async (e: any) => {
      console.log(`New follower: ${e.user.did}`);
      await processUser(e.user.did, labeler);
    });

    bot.on("like", async (e: any) => {
      console.log(`Like detected from: ${e.user.did}`);
      await processUser(e.user.did, labeler);
    });

  } catch (e) {
    console.error("Bot login failed:", e);
  }
}

main();
