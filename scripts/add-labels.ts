
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

    // ラベル定義を追加 (com.atproto.repo.putRecord を直接叩く)
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
                        "daikichi",
                        "kichi",
                        "chukichi",
                        "shokichi",
                        "suekichi",
                        "kyo",
                        "daikyo",
                    ],
                    labelValueDefinitions: [
                        {
                            identifier: "daikichi",
                            severity: "inform",
                            blurs: "none",
                            defaultSetting: "warn",
                            locales: [
                                { lang: "ja", name: "大吉", description: "今日の運勢は大吉です！最高の一日になりそう。" },
                                { lang: "en", name: "Daikichi", description: "Great Blessing! You will have an excellent day." },
                            ],
                        },
                        {
                            identifier: "kichi",
                            severity: "inform",
                            blurs: "none",
                            defaultSetting: "warn",
                            locales: [
                                { lang: "ja", name: "吉", description: "今日の運勢は吉です。良いことあるかも。" },
                                { lang: "en", name: "Kichi", description: "Blessing. Good things might happen." },
                            ],
                        },
                        {
                            identifier: "chukichi",
                            severity: "inform",
                            blurs: "none",
                            defaultSetting: "warn",
                            locales: [
                                { lang: "ja", name: "中吉", description: "今日の運勢は中吉です。そこそこ良い感じです。" },
                                { lang: "en", name: "Chukichi", description: "Middle Blessing. Not bad at all." },
                            ],
                        },
                        {
                            identifier: "shokichi",
                            severity: "inform",
                            blurs: "none",
                            defaultSetting: "warn",
                            locales: [
                                { lang: "ja", name: "小吉", description: "今日の運勢は小吉です。ささやかな幸せを大切に。" },
                                { lang: "en", name: "Shokichi", description: "Small Blessing. Cherish the little things." },
                            ],
                        },
                        {
                            identifier: "suekichi",
                            severity: "inform",
                            blurs: "none",
                            defaultSetting: "warn",
                            locales: [
                                { lang: "ja", name: "末吉", description: "今日の運勢は末吉です。これから良くなるはず。" },
                                { lang: "en", name: "Suekichi", description: "Ending Blessing. Things will get better." },
                            ],
                        },
                        {
                            identifier: "kyo",
                            severity: "inform",
                            blurs: "none",
                            defaultSetting: "warn",
                            locales: [
                                { lang: "ja", name: "凶", description: "今日の運勢は凶です。気を引き締めていきましょう。" },
                                { lang: "en", name: "Kyo", description: "Curse. Stay alert." },
                            ],
                        },
                        {
                            identifier: "daikyo",
                            severity: "inform",
                            blurs: "none",
                            defaultSetting: "warn",
                            locales: [
                                { lang: "ja", name: "大凶", description: "今日の運勢は大凶です。無理せず慎重に。" },
                                { lang: "en", name: "Daikyo", description: "Great Curse. Be very careful today." },
                            ],
                        },
                    ],
                },
            },
        },
    });

    console.log("Label definitions added!");
})();
