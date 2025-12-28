import dotenv from "dotenv";
import { LabelerServer } from "@skyware/labeler";
import { Bot } from "@skyware/bot";
import Database from "better-sqlite3";
import fs from "node:fs";
import path from "node:path";
import { getDailyFortune, FORTUNES } from "./fortune.js";

dotenv.config();

const PORT = parseInt(process.env.PORT || "3000");
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
 * ラベルを付与する（古い運勢は剥がす）
 * @param did アクションを起こしたユーザーの DID
 */
async function processUser(did: string) {
  const fortune = getDailyFortune(did);
  const negateList = FORTUNES.map(f => f.val).filter(v => v !== fortune);
  console.log(`Processing ${did}: ${fortune} (Negating: ${negateList.join(", ")})`);

  try {
    // 古い運勢を剥がしつつ、新しい運勢を付与
    await labeler.createLabels(
      { uri: did },
      {
        create: [fortune],
        negate: negateList,
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

/**
 * ユーザーから全ての運勢ラベルを剥奪し、ローカルDBからも削除します。
 */
async function negateUser(did: string) {
  console.log(`Opt-out cleanup: Removing labels for ${did}`);
  const allFortunes = FORTUNES.map((f) => f.val);
  try {
    await labeler.createLabels({ uri: did }, { negate: allFortunes });
    db.prepare("DELETE FROM labels WHERE uri = ?").run(did);
  } catch (e) {
    console.error(`Failed to cleanup ${did}:`, e);
  }
}

async function runBatch() {
  console.log("[Batch] Starting daily update...");

  // 1. 全フォロワー取得
  const currentFollowers = new Set<string>();
  let cursor: string | undefined;
  try {
    do {
      const response: any = await (bot.agent as any).get("app.bsky.graph.getFollowers", {
        params: {
          actor: process.env.LABELER_DID,
          limit: 100,
          cursor,
        },
      });
      if (response.data.followers) {
        for (const f of response.data.followers) {
          currentFollowers.add(f.did);
        }
      }
      cursor = response.data.cursor;
      await new Promise(r => setTimeout(r, 100)); // Rate limit
    } while (cursor);
    console.log(`[Batch] Fetched ${currentFollowers.size} followers.`);
  } catch (e) {
    console.error("[Batch] Failed to fetch followers, aborting batch:", e);
    return;
  }

  // 2. DBユーザーとの比較
  const rows = db.prepare("SELECT DISTINCT uri FROM labels WHERE uri LIKE 'did:%'").all() as { uri: string }[];
  console.log(`[Batch] Local DB has ${rows.length} users.`);

  for (const row of rows) {
    if (currentFollowers.has(row.uri)) {
      // まだフォロー中 -> 更新
      await processUser(row.uri);
    } else {
      // フォロー外した -> 削除
      await negateUser(row.uri);
    }
    await new Promise(r => setTimeout(r, 50));
  }

  // 3. (Optional) DBになくてフォロー中のユーザーを救済する場合はここで処理
  // 今回は「アンフォロー対応」が主目的なので、既存ロジックに合わせてDBベースのループを主としました。

  console.log("[Batch] Complete.");
}

function startMidnightScheduler() {
  // 起動時に一度実行（アンフォロー反映のため）
  setTimeout(() => runBatch().catch(console.error), 10000); // ログイン待ちで10秒後

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
      await runBatch();
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
