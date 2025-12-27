import { LabelerServer } from "@skyware/labeler";
import { FORTUNES, getDailyFortune, getRandom3Labels, OLD_LABELS, NEW_LABELS } from "./fortune.js";
import { db } from "./db.js";

/**
 * 指定された運勢以外の全ての運勢リスト（Negate対象）を返します。
 * @param currentFortune 現在の運勢
 * @returns 打ち消すべき運勢ラベルのリスト
 */
export function calculateNegateList(currentFortune: string): string[] {
    return FORTUNES.map((f) => f.val).filter((v) => v !== currentFortune);
}

/**
 * ユーザーに対してランダムで3つのラベルを付与し、それ以外（旧ラベル含む）を全て打ち消します。
 * @param did 対象ユーザーのDID
 * @param labeler LabelerServerのインスタンス
 */
export async function processUser(did: string, labeler: LabelerServer) {
    const selectedLabels = getRandom3Labels();
    console.log(`Processing ${did}, selected: ${selectedLabels.join(", ")}`);

    // Negate all old labels AND any new labels not currently selected
    const negateList = [
        ...OLD_LABELS,
        ...NEW_LABELS.filter(l => !selectedLabels.includes(l))
    ];

    try {
        await labeler.createLabels(
            { uri: did },
            {
                create: selectedLabels,
                negate: negateList,
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
 */
export async function negateUser(did: string, labeler: LabelerServer) {
    console.log(`Opt-out cleanup: Removing labels for ${did}`);
    const allFortunes = FORTUNES.map((f) => f.val);
    try {
        await labeler.createLabels({ uri: did }, { negate: allFortunes });
        db.prepare("DELETE FROM labels WHERE uri = ?").run(did);
    } catch (e) {
        console.error(`Failed to cleanup ${did}:`, e);
    }
}
