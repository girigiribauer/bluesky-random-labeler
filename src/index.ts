import dotenv from "dotenv";
import { LabelerServer } from "@skyware/labeler";
import { createHash } from "node:crypto";

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

(async () => {
  dotenv.config();

  const server = new LabelerServer({
    did: process.env.LABELER_DID ?? "",
    signingKey: process.env.SIGNING_KEY ?? "",
    dbPath: ":memory:",
  });

  server.start({ port: 4000, host: "0.0.0.0" }, (error) => {
    if (error) {
      console.error("Failed to start server:", error);
    } else {
      console.log("Labeler server running on port 4000");
    }
  });

  server.queryLabelsHandler = async (req: any, res: any) => {
    const { uriPatterns } = req.query;
    const labels = [];

    if (uriPatterns) {
      const subjects = Array.isArray(uriPatterns) ? uriPatterns : [uriPatterns];

      for (const uri of subjects) {
        try {
          const fortune = getDailyFortune(uri);
          console.log(`Providing fortune for ${uri}: ${fortune}`);

          const label = await server.createLabel({
            uri,
            val: fortune,
          });
          labels.push(label);
        } catch (e) {
          console.error(`Failed to create label for ${uri}`, e);
        }
      }
    }

    return res.send({
      cursor: "0",
      labels,
    });
  };
})();
