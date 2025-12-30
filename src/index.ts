import dotenv from "dotenv";
import { LabelerServer } from "@skyware/labeler";
import { Bot } from "@skyware/bot";
import { processUser } from "./labeling.js";
import { startMidnightScheduler } from "./scheduler.js";

dotenv.config();

const PORT = parseInt(process.env.PORT || "3000");
const DB_PATH = process.env.DB_PATH || "data/labels.db";

const labeler = new LabelerServer({
  did: process.env.LABELER_DID ?? "",
  signingKey: process.env.SIGNING_KEY ?? "",
  dbPath: DB_PATH,
});

const bot: Bot = new Bot();

async function startNotificationPolling() {
  try {
    await bot.login({
      identifier: process.env.LABELER_DID ?? "",
      password: process.env.LABELER_PASSWORD ?? "",
    });
    console.log("Bot logged in for notification polling.");

    startMidnightScheduler(bot, labeler);

    bot.on("follow", async (e: any) => {
      console.log(`New follower: ${e.user.did}`);
      await processUser(e.user.did, labeler, e.user.handle);
    });

    bot.on("like", async (e: any) => {
      console.log(`New like from: ${e.user.did}`);
      await processUser(e.user.did, labeler, e.user.handle);
    });

  } catch (e) {
    console.error("Failed to login/start polling:", e);
  }
}

labeler.start({ port: PORT, host: "0.0.0.0" }, (error) => {
  if (error) {
    console.error("Failed to start server", error);
  } else {
    console.log(`Labeler running on port ${PORT}`);
    startNotificationPolling();
  }
});
