
import { Bot } from "@skyware/bot";
import dotenv from "dotenv";

(async () => {
    dotenv.config();

    const bot = new Bot();
    await bot.login({
        identifier: process.env.LABELER_DID ?? "",
        password: process.env.LABELER_PASSWORD ?? "",
    });

    console.log("Adding label definitions...");

    console.log("Sending putRecord request...");
    await bot.agent.call("com.atproto.repo.putRecord", {
        data: {
            repo: bot.profile.did,
            collection: "app.bsky.labeler.service",
            rkey: "self",
            record: {
                $type: "app.bsky.labeler.service",
                createdAt: new Date().toISOString(),
                policies: {
                    labelValues: [
                        "A", "B", "C", "D", "E",
                        "1", "2", "3", "4", "5"
                    ],
                    labelValueDefinitions: [
                        { identifier: "A", severity: "inform", blurs: "none", defaultSetting: "warn", locales: [{ lang: "en", name: "Label A", description: "Test Label A" }] },
                        { identifier: "B", severity: "inform", blurs: "none", defaultSetting: "warn", locales: [{ lang: "en", name: "Label B", description: "Test Label B" }] },
                        { identifier: "C", severity: "inform", blurs: "none", defaultSetting: "warn", locales: [{ lang: "en", name: "Label C", description: "Test Label C" }] },
                        { identifier: "D", severity: "inform", blurs: "none", defaultSetting: "warn", locales: [{ lang: "en", name: "Label D", description: "Test Label D" }] },
                        { identifier: "E", severity: "inform", blurs: "none", defaultSetting: "warn", locales: [{ lang: "en", name: "Label E", description: "Test Label E" }] },
                        { identifier: "1", severity: "inform", blurs: "none", defaultSetting: "warn", locales: [{ lang: "en", name: "Label 1", description: "Test Label 1" }] },
                        { identifier: "2", severity: "inform", blurs: "none", defaultSetting: "warn", locales: [{ lang: "en", name: "Label 2", description: "Test Label 2" }] },
                        { identifier: "3", severity: "inform", blurs: "none", defaultSetting: "warn", locales: [{ lang: "en", name: "Label 3", description: "Test Label 3" }] },
                        { identifier: "4", severity: "inform", blurs: "none", defaultSetting: "warn", locales: [{ lang: "en", name: "Label 4", description: "Test Label 4" }] },
                        { identifier: "5", severity: "inform", blurs: "none", defaultSetting: "warn", locales: [{ lang: "en", name: "Label 5", description: "Test Label 5" }] },
                    ],
                },
            },
        },
    });

    console.log("Label definitions added!");
})();
