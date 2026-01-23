import { LabelerServer } from "@skyware/labeler";
import { FORTUNES, getDailyFortune } from "./fortune.js";
import { getJstTime } from "./utils.js";
// db import removed to keep this file pure/testable

/**
 * 指定された運勢以外の全ての運勢リスト（Negate対象）を返します。
 * @param currentFortune 現在の運勢
 * @returns 打ち消すべき運勢ラベルのリスト
 */
export function calculateNegateList(currentFortune: string): string[] {
    return FORTUNES.map((f) => f.val).filter((v) => v !== currentFortune);
}

/**
 * ユーザーに対して特定の運勢ラベルを付与し、それ以外の運勢ラベルを全て打ち消します (Negate)。
 * ギミック等の強制上書き用です。
 * @param did 対象ユーザーのDID
 * @param fortune 付与する運勢
 * @param labeler LabelerServerのインスタンス
 */
export async function overwriteFortune(did: string, fortune: string, labeler: LabelerServer) {
    const negate = calculateNegateList(fortune);
    try {
        await labeler.createLabels(
            { uri: did },
            {
                create: [fortune],
                negate: negate,
            }
        );
    } catch (e) {
        console.error(`Error applying fortune ${fortune} to ${did}:`, e);
    }
}

/**
 * ユーザーに対して日替わりの運勢ラベルを付与し、それ以外の運勢ラベルを全て打ち消します (Negate)。
 * @param did 対象ユーザーのDID
 * @param labeler LabelerServerのインスタンス
 */
export async function processUser(did: string, labeler: LabelerServer, handle?: string) {
    const fortune = getDailyFortune(did);
    const now = getJstTime();
    const identifier = handle ? `${handle} (${did})` : did;
    console.log(`[${now}] Processing ${identifier}, fortune: ${fortune}`);

    const negate = calculateNegateList(fortune);

    try {
        await labeler.createLabels(
            { uri: did },
            {
                create: [fortune],
                negate: negate,
            }
        );
    } catch (e) {
        console.error(`Error processing user ${did}:`, e);
    }
}

/**
 * ユーザーから全ての運勢ラベルを剥奪し (Opt-out)、ローカルDBからも削除します。
 * @param did 対象ユーザーのDID
 * @param labeler LabelerServerのインスタンス
 * @param db Databaseインスタンス (Dependency Injection)
 */
export async function negateUser(did: string, labeler: LabelerServer, db: any) {
    const now = getJstTime();
    console.log(`[${now}] Cleanup: Removing labels from ${did}`);
    const allFortunes = FORTUNES.map((f) => f.val);
    try {
        await labeler.createLabels({ uri: did }, { negate: allFortunes });
        db.prepare("DELETE FROM labels WHERE uri = ?").run(did);
    } catch (e) {
        console.error(`Failed to cleanup ${did}:`, e);
    }
}
