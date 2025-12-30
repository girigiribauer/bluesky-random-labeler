import { describe, it, expect } from "vitest";
import { getJstDate, getJstTime } from "../src/utils.js";

describe("getJstDate", () => {
    it("常に 'YYYY-MM-DD' 形式の文字列を返す", () => {
        const result = getJstDate();
        expect(result).toMatch(/^\d{4}-\d{2}-\d{2}$/);
        expect(result).not.toContain("T"); // 時刻が含まれていないこと
        expect(result).not.toContain("/"); // スラッシュ区切りでないこと
    });

    it("UTCで日付が前日でも、日本時間で計算される", () => {
        // UTC: 2024-12-31 15:00:00 -> JST: 2025-01-01 00:00:00
        const dateUtc = new Date("2024-12-31T15:00:00Z");
        const result = getJstDate(dateUtc);
        expect(result).toBe("2025-01-01");
    });

    it("午後2時など、日中の任意の時間でも正しく日付だけを返す", () => {
        const date = new Date("2025-10-10T14:30:00+09:00");
        const result = getJstDate(date);
        expect(result).toBe("2025-10-10");
    });
});

describe("getJstTime", () => {
    it("常に 'YYYY-MM-DD HH:mm:ss' 形式の文字列を返す", () => {
        const result = getJstTime();
        expect(result).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/);
    });

    it("UTCで日時が変換され、正しい日本時間になる", () => {
        // UTC: 2024-12-31 15:00:00 -> JST: 2025-01-01 00:00:00
        const dateUtc = new Date("2024-12-31T15:00:00Z");
        const result = getJstTime(dateUtc);
        expect(result).toBe("2025-01-01 00:00:00");
    });

    it("特定の時刻が正しくフォーマットされる", () => {
        // JST: 2025-10-10 14:30:05
        const date = new Date("2025-10-10T14:30:05+09:00");
        const result = getJstTime(date);
        expect(result).toBe("2025-10-10 14:30:05");
    });
});
