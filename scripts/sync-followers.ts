import dotenv from "dotenv";
import { Bot } from "@skyware/bot";
import { LabelerServer } from "@skyware/labeler";
import fs from "node:fs";
import path from "node:path";
import { getDailyFortune, FORTUNES } from "../src/fortune.js";

dotenv.config();

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

const bot = new Bot();

(async () => {
    console.log("== Starting Follower Sync Script ==");
    console.log(`DB Path: ${DB_PATH}`);
    console.log(`Labeler DID: ${process.env.LABELER_DID}`);

    await bot.login({
        identifier: process.env.LABELER_DID ?? "",
        password: process.env.LABELER_PASSWORD ?? "",
    });
    console.log("Bot logged in.");

    let cursor: string | undefined;
    let count = 0;

    try {
        do {
            const agent = bot.agent as any;
            const response = await agent.get("app.bsky.graph.getFollowers", {
                params: {
                    actor: process.env.LABELER_DID ?? "",
                    cursor,
                    limit: 100,
                },
            });

            for (const subject of response.data.followers) {
                const did = subject.did;
                const fortune = getDailyFortune(did);
                const allFortunes = FORTUNES.map(f => f.val);
                const negate = allFortunes.filter(v => v !== fortune);

                // Use labeler.createLabels to persist and negate old ones
                try {
                    await labeler.createLabels({ uri: did }, {
                        create: [fortune],
                        negate: negate,
                    });
                    // Only log every 10 users to reduce noise
                    if (count % 10 === 0) process.stdout.write(".");
                } catch (e: any) {
                    console.error(`\nFailed to label ${did}: ${e.message}`);
                }
                count++;
            }

            cursor = response.data.cursor;
        } while (cursor);

        console.log(`\nSync complete. Processed ${count} users.`);
    } catch (e) {
        console.error("\nSync failed:", e);
        process.exit(1);
    }
})();
