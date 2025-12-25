import dotenv from "dotenv";
import { LabelerServer } from "@skyware/labeler";
import { Bot } from "@skyware/bot";
import Database from "better-sqlite3";
import fs from "node:fs";
import path from "node:path";
import { getDailyFortune, FORTUNES } from "./fortune.js";

dotenv.config();

const PORT = parseInt(process.env.PORT || "4000");
const DB_PATH = process.env.DB_PATH || "data/labels.db";
const dataDir = path.dirname(DB_PATH);
if (!fs.existsSync(dataDir)) {
  fs.mkdirSync(dataDir, { recursive: true });
}

const labeler = new LabelerServer({
  did: process.env.LABELER_DID ?? "",
  signingKey: process.env.SIGNING_KEY ?? "",
  dbPath: DB_PATH,
});

const db = new Database(DB_PATH);

/**
 * 今日の運勢のラベルを付与する
 * @param did アクションを起こしたユーザーの DID
 */
async function processUser(did: string) {
  const fortune = getDailyFortune(did);
  const allFortunes = FORTUNES.map((f) => f.val);
  const negate = allFortunes.filter((v) => v !== fortune);

  console.log(`Processing ${did}: ${fortune}`);

  try {
    await labeler.createLabels(
      { uri: did },
      {
        create: [fortune],
        negate,
      }
    );
  } catch (e) {
    console.error(`Failed to label ${did}:`, e);
  }
}

const bot: Bot = new Bot();

async function startNotificationPolling() {
  try {
    await bot.login({
      identifier: process.env.LABELER_DID ?? "",
      password: process.env.LABELER_PASSWORD ?? "",
    });
    console.log("Bot logged in for notification polling.");

    bot.on("follow", async (e: any) => {
      console.log(`New follower: ${e.user.did}`);
      await processUser(e.user.did);
    });

    bot.on("like", async (e: any) => {
      console.log(`New like from: ${e.user.did}`);
      await processUser(e.user.did);
    });

  } catch (e) {
    console.error("Failed to login/start polling:", e);
  }
}

function startMidnightScheduler() {
  let lastDay = new Date().toLocaleString("en-US", { timeZone: "Asia/Tokyo" }).split("T")[0];

  setInterval(async () => {
    const jstDate = new Date(new Date().toLocaleString("en-US", { timeZone: "Asia/Tokyo" }));

    const year = jstDate.getFullYear();
    const month = String(jstDate.getMonth() + 1).padStart(2, '0');
    const day = String(jstDate.getDate()).padStart(2, '0');
    const todayJst = `${year}-${month}-${day}`;

    if (todayJst !== lastDay) {
      console.log(`Midnight detected! ${lastDay} -> ${todayJst}. Running batch...`);
      lastDay = todayJst;

      const rows = db.prepare("SELECT DISTINCT uri FROM labels WHERE uri LIKE 'did:%'").all() as { uri: string }[];
      console.log(`found ${rows.length} users to update.`);

      for (const row of rows) {
        await processUser(row.uri);
        await new Promise(r => setTimeout(r, 50));
      }
      console.log("Batch complete.");
    }
  }, 60000);
}

labeler.start({ port: PORT, host: "0.0.0.0" }, (error) => {
  if (error) {
    console.error("Failed to start server", error);
  } else {
    console.log(`Labeler running on port ${PORT}`);
    startNotificationPolling();
    startMidnightScheduler();
  }
});
