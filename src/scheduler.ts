import { Bot } from "@skyware/bot";
import { LabelerServer } from "@skyware/labeler";
import { db as defaultDb } from "./db.js";
import { processUser, negateUser } from "./labeling.js";
import { getJstDate, getJstTime } from "./utils.js";
import Database from "better-sqlite3";

/**
 * 深夜にバッチを起動し、運勢の更新とフォロー解除者のクリーンアップを行います。
 * 最適化: フォロワー・フォロイーの一括取得を行い、API呼び出し回数を最小限に抑えます。
 */
export function startMidnightScheduler(bot: Bot, labeler: LabelerServer, db: Database.Database = defaultDb) {
    let lastDay = getJstDate();

    // 起動時に即座にバッチを実行 (for testing / recovery)
    console.log("Starting initial batch run...");
    runOptimizedBatch(bot, labeler, db).catch(console.error);

    setInterval(async () => {
        const todayJst = getJstDate();

        // 日付変更を検知 (JST 0:00)
        if (todayJst !== lastDay) {
            console.log(`Midnight detected! ${lastDay} -> ${todayJst}. Running optimized batch...`);
            lastDay = todayJst;

            try {
                await runOptimizedBatch(bot, labeler, db);
            } catch (e) {
                console.error("Batch execution failed:", e);
            }

            console.log("Batch complete.");
        }
    }, 60000); // 1分ごとにチェック
}

export async function runOptimizedBatch(bot: Bot, labeler: LabelerServer, db: Database.Database = defaultDb) {
    const log = (msg: string) => console.log(`[${getJstTime()}] ${msg}`);
    const errorLog = (msg: string, e: any) => console.error(`[${getJstTime()}] ${msg}`, e);

    // 1. ローカルDBから追跡中の全ユーザーを取得
    const rows = db.prepare("SELECT DISTINCT uri FROM labels WHERE uri LIKE 'did:%'").all() as { uri: string }[];
    const localDids = new Set(rows.map(r => r.uri));
    log(`[Batch] Found ${localDids.size} users in local DB.`);

    // 2. 現在の全フォロワーをAPIから取得
    log("[Batch] Fetching current followers from API...");
    const currentFollowers = new Map<string, string>(); // did -> handle
    let cursor: string | undefined;

    do {
        try {
            // 最適化: 100件ずつ取得
            const response = await (bot.agent as any).get("app.bsky.graph.getFollowers", {
                params: {
                    actor: bot.profile?.did ?? "",
                    cursor,
                    limit: 100,
                },
            });

            if (response.data.followers) {
                for (const f of response.data.followers) {
                    currentFollowers.set(f.did, f.handle);
                }
            }
            cursor = response.data.cursor;
            // レート制限保護
            await new Promise(r => setTimeout(r, 100));
        } catch (e) {
            errorLog("[Batch] Failed to fetch followers chunk:", e);
            throw e; // API失敗時は処理を中断し、不完全なリストでの削除を防ぐ
        }
    } while (cursor);

    log(`[Batch] Fetched ${currentFollowers.size} active followers.`);

    // 3. 比較と処理
    let updateCount = 0;
    let removeCount = 0;

    // A. フォロー中のユーザー: 全員処理 (DBにない場合も復活させる)
    for (const [did, handle] of currentFollowers) {
        await processUser(did, labeler, handle);
        updateCount++;
        // 負荷分散
        await new Promise(r => setTimeout(r, 50));
    }

    // B. DBにはいるがフォローしていないユーザー: クリーンアップ
    for (const did of localDids) {
        if (!currentFollowers.has(did)) {
            await negateUser(did, labeler, db);
            removeCount++;
            await new Promise(r => setTimeout(r, 50));
        }
    }

    log(`[Batch] Summary: Updated ${updateCount}, Removed ${removeCount}.`);
}
