import dotenv from "dotenv";
import { LabelerServer } from "@skyware/labeler";
import { Bot } from "@skyware/bot";
import { processUser, overwriteFortune } from "./labeling.js";
import { FORTUNES } from "./fortune.js";
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

setTimeout(() => {
  console.log("[INIT] Attempting to register custom routes via setTimeout workaround...");

  labeler.app.post("/xrpc/com.atproto.moderation.createReport", async (req, reply) => {
    console.log("[HANDLER] Handling createReport request");
    const { reasonType, reason, subject } = req.body as any;
    console.log("Received Report:", { reasonType, reason, subject });

    // Extract reportedBy from Authorization header
    let reportedBy = "did:plc:unknown";
    const authHeader = req.headers.authorization;
    if (authHeader) {
      const [, token] = authHeader.split(" ");
      if (token) {
        try {
          const payload = JSON.parse(Buffer.from(token.split(".")[1], "base64url").toString());
          reportedBy = payload.iss || "did:plc:unknown";
        } catch (e) {
          console.error("Failed to decode JWT:", e);
        }
      }
    }

    const GIMMICK_KEYWORDS = FORTUNES
      .map(f => ({ val: f.val, keywords: [f.val, f.label] }))
      .sort((a, b) => {
        const maxLenA = Math.max(...a.keywords.map(k => k.length));
        const maxLenB = Math.max(...b.keywords.map(k => k.length));
        return maxLenB - maxLenA;
      });

    if (reason) {
      for (const gimmick of GIMMICK_KEYWORDS) {
        if (gimmick.keywords.some(k => reason.includes(k))) {
          console.log(`Gimmick Triggered! Forcing ${gimmick.val} for:`, subject.did);

          await overwriteFortune(subject.did, gimmick.val, labeler);
          break;
        }
      }
    }

    return {
      id: 12345,
      reasonType,
      reason,
      subject,
      reportedBy,
      createdAt: new Date().toISOString(),
    };
  });

  console.log("[INIT] Custom route added. Starting server...");

  labeler.start({ port: PORT, host: "0.0.0.0" }, (error) => {
    if (error) {
      console.error("Failed to start server", error);
    } else {
      console.log(`Labeler running on port ${PORT}`);
      startNotificationPolling();
    }
  });
}, 3000);
