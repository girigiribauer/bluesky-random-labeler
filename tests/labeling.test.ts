import { describe, it, expect } from "vitest";
import { calculateNegateList } from "../src/labeling.js";
import { FORTUNES } from "../src/fortune.js";

describe("calculateNegateList", () => {
    it("現在の運勢以外の全ての運勢が含まれていること", () => {
        const currentFortune = "daikichi";
        const result = calculateNegateList(currentFortune);

        expect(result).not.toContain("daikichi"); // 自分自身は含まない
        expect(result).toContain("kichi");
        expect(result).toContain("chukichi");
        expect(result).toContain("daikyo");

        // 全体の数 - 1 (自分) = 結果の数
        expect(result.length).toBe(FORTUNES.length - 1);
    });

    it("無効な運勢が渡された場合、全ての運勢リストが返る", () => {
        // 万が一、定義外の文字列が入った場合は「全て否定」になる（安全側に倒れる）
        const result = calculateNegateList("invalid_fortune");
        expect(result.length).toBe(FORTUNES.length);
        expect(result).toContain("daikichi");
    });
});
