import { Bot } from "@skyware/bot";
import { LabelerServer } from "@skyware/labeler";
import { db } from "./db.js";
import { processUser, negateUser } from "./labeling.js";
import { getJstDate } from "./utils.js";

/**
 * 深夜にバッチを起動し、運勢の更新とフォロー解除者のクリーンアップを行います。
 * 最適化: フォロワー・フォロイーの一括取得を行い、API呼び出し回数を最小限に抑えます。
 */
export function startMidnightScheduler(bot: Bot, labeler: LabelerServer) {
    let lastDay = getJstDate();

    setInterval(async () => {
        const todayJst = getJstDate();

        // 日付変更を検知 (JST 0:00)
        if (todayJst !== lastDay) {
            console.log(`Midnight detected! ${lastDay} -> ${todayJst}. Running optimized batch...`);
            lastDay = todayJst;

            try {
                await runOptimizedBatch(bot, labeler);
            } catch (e) {
                console.error("Batch execution failed:", e);
            }

            console.log("Batch complete.");
        }
    }, 60000); // 1分ごとにチェック
}

async function runOptimizedBatch(bot: Bot, labeler: LabelerServer) {
    // 1. ローカルDBから追跡中の全ユーザーを取得
    const rows = db.prepare("SELECT DISTINCT uri FROM labels WHERE uri LIKE 'did:%'").all() as { uri: string }[];
    const localDids = new Set(rows.map(r => r.uri));
    console.log(`[Batch] Found ${localDids.size} users in local DB.`);

    // 2. 現在の全フォロワーをAPIから取得
    console.log("[Batch] Fetching current followers from API...");
    const currentFollowers = new Set<string>();
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
                    currentFollowers.add(f.did);
                }
            }
            cursor = response.data.cursor;
            // レート制限保護
            await new Promise(r => setTimeout(r, 100));
        } catch (e) {
            console.error("[Batch] Failed to fetch followers chunk:", e);
            break; // 状態不整合を防ぐため、APIエラー時は取得を中断
        }
    } while (cursor);

    console.log(`[Batch] Fetched ${currentFollowers.size} active followers.`);

    // 3. 比較と処理
    let updateCount = 0;
    let removeCount = 0;

    for (const did of localDids) {
        if (currentFollowers.has(did)) {
            // まだフォローしている -> 運勢を更新
            await processUser(did, labeler);
            updateCount++;
        } else {
            // フォロワーリストにいない -> オプトアウト (クリーンアップ)
            await negateUser(did, labeler);
            removeCount++;
        }
        // Labeler APIへの負荷を考慮した短い遅延
        await new Promise(r => setTimeout(r, 50));
    }

    console.log(`[Batch] Summary: Updated ${updateCount}, Removed ${removeCount}.`);
}
