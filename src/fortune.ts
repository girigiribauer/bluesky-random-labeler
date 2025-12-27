import { createHash } from "node:crypto";
import { getJstDate } from "./utils.js";

// Old labels for cleanup
export const OLD_LABELS = [
    "daikichi", "kichi", "chukichi", "shokichi", "suekichi", "kyo", "daikyo"
];

// New test labels
export const NEW_LABELS = [
    "A", "B", "C", "D", "E",
    "1", "2", "3", "4", "5"
];

export const FORTUNES = [
    { val: "daikichi", threshold: 6 },   // 6%
    { val: "kichi", threshold: 28 },     // 22%
    { val: "chukichi", threshold: 50 },  // 22%
    { val: "shokichi", threshold: 70 },  // 20%
    { val: "suekichi", threshold: 88 },  // 18%
    { val: "kyo", threshold: 97 },       // 9%
    { val: "daikyo", threshold: 100 },   // 3%
];

export function getDailyFortune(did: string, date?: Date): string {
    const dateStr = getJstDate(date);

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

export function getRandom3Labels(): string[] {
    // Shuffle copy of NEW_LABELS
    const shuffled = [...NEW_LABELS].sort(() => 0.5 - Math.random());
    return shuffled.slice(0, 3);
}
