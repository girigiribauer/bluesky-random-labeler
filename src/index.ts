import dotenv from "dotenv";
import { LabelerServer } from "@skyware/labeler";
import { Bot, Profile } from "@skyware/bot";

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
          const label = await server.createLabel({
            uri,
            val: "cool-cat",
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
