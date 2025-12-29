import { describe, it, expect, vi } from "vitest";
import { calculateNegateList, processUser, negateUser } from "../src/labeling.js";
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

describe("processUser", () => {
    it("現在の運勢をcreateし、それ以外をnegateするAPIリクエストを送信する", async () => {
        const mockLabeler = {
            createLabels: vi.fn(),
        } as any;
        const did = "did:plc:testuser";

        await processUser(did, mockLabeler);

        expect(mockLabeler.createLabels).toHaveBeenCalledTimes(1);
        const args = mockLabeler.createLabels.mock.calls[0];

        // 第1引数: { uri: did }
        expect(args[0]).toEqual({ uri: did });

        // 第2引数: { create: [...], negate: [...] }
        const opts = args[1];
        expect(opts.create.length).toBe(1); // 1つ作成
        expect(opts.negate.length).toBe(FORTUNES.length - 1); // 残りは否定
        // createに含まれる運勢が、negateに含まれていないこと
        const createdFortune = opts.create[0];
        expect(opts.negate).not.toContain(createdFortune);
    });
});

describe("negateUser", () => {
    it("全ての運勢をnegateし、DBからも削除する", async () => {
        const mockLabeler = {
            createLabels: vi.fn(),
        } as any;

        const mockRun = vi.fn();
        const mockDb = {
            prepare: vi.fn().mockReturnValue({ run: mockRun }),
        } as any; // Mock Database

        const did = "did:plc:leavinguser";

        await negateUser(did, mockLabeler, mockDb);

        // 1. Labelerで全否定
        expect(mockLabeler.createLabels).toHaveBeenCalledTimes(1);
        const opts = mockLabeler.createLabels.mock.calls[0][1];
        expect(opts.negate.length).toBe(FORTUNES.length); // 全種類Negate
        expect(opts.create).toBeUndefined(); // createは無し

        // 2. DBから削除
        expect(mockDb.prepare).toHaveBeenCalledWith(expect.stringContaining("DELETE FROM labels"));
        expect(mockRun).toHaveBeenCalledWith(did);
    });
});
