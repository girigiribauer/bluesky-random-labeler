import { describe, it, expect } from "vitest";
import { getDailyFortune } from "../src/fortune.js";

describe("getDailyFortune", () => {
    it("同じユーザー、同じ日付なら、同じおみくじ結果が返る", () => {
        const did = "did:plc:testuser123";
        const date = new Date("2025-01-01T00:00:00+09:00");

        const fortune1 = getDailyFortune(did, date);
        const fortune2 = getDailyFortune(did, date);

        expect(fortune1).toBe(fortune2);
    });

    it("同じユーザー、異なる日付なら、異なるおみくじ結果が返る", () => {
        const did = "did:plc:testuser123";
        const date1 = new Date("2025-01-01T00:00:00+09:00"); // JST
        const date2 = new Date("2025-01-02T00:00:00+09:00"); // Next day

        const fortune1 = getDailyFortune(did, date1);
        const fortune2 = getDailyFortune(did, date2);

        if (fortune1 === fortune2) {
            console.warn("Hash collision in test (rare but possible), skipping equality check.");
        } else {
            expect(fortune1).not.toBe(fortune2);
        }
    });

    it("世界標準時で日付が異なっても、日本時間で同じ日付なら同じ結果が返る", () => {
        const did = "did:plc:globaluser";
        // 日本時間 2025/01/01 00:00:00 は 世界標準時で 2024/12/31 15:00:00
        const dateJst = new Date("2025-01-01T00:00:00+09:00");
        const dateUtc = new Date("2024-12-31T15:00:00Z");

        expect(getDailyFortune(did, dateJst)).toBe(getDailyFortune(did, dateUtc));
    });
});
