import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { runOptimizedBatch, startMidnightScheduler } from '../src/scheduler';
import { LabelerServer } from '@skyware/labeler';
import { Bot } from '@skyware/bot';
import * as labeling from '../src/labeling';
import * as utils from '../src/utils';

// Mock dependencies
vi.mock('../src/labeling', () => ({
    processUser: vi.fn(),
    negateUser: vi.fn(),
}));

vi.mock('../src/utils', () => ({
    getJstDate: vi.fn(),
    getJstTime: vi.fn().mockReturnValue('2024-01-01 00:00:00'),
}));

describe('runOptimizedBatch', () => {
    let mockBot: any;
    let mockLabeler: any;
    let mockDb: any;

    beforeEach(() => {
        vi.clearAllMocks();

        mockBot = {
            profile: { did: 'did:bot' },
            agent: {
                get: vi.fn(),
            },
        } as unknown as Bot;

        mockLabeler = {} as unknown as LabelerServer;

        mockDb = {
            prepare: vi.fn(),
        };
    });

    it('フォロワーを維持する（processUserを呼び出す）', async () => {
        // Setup DB: User exists
        mockDb.prepare.mockReturnValue({
            all: () => [{ uri: 'did:user:keep' }],
        });

        // Setup Bot: User is still following
        mockBot.agent.get.mockResolvedValue({
            data: {
                followers: [{ did: 'did:user:keep', handle: 'handle.keep' }],
                cursor: undefined
            }
        });

        await runOptimizedBatch(mockBot, mockLabeler, mockDb);

        // Verify: processUser should be called for active follower
        expect(labeling.processUser).toHaveBeenCalledWith('did:user:keep', mockLabeler, 'handle.keep');
        // Verify: negateUser should NOT be called (user is safe)
        expect(labeling.negateUser).not.toHaveBeenCalled();
    });

    it('非フォロワーを削除する（negateUserを呼び出す）', async () => {
        // Setup DB: User exists (was tracked)
        mockDb.prepare.mockReturnValue({
            all: () => [{ uri: 'did:user:leave' }],
        });

        // Setup Bot: Follower list is empty (User left)
        mockBot.agent.get.mockResolvedValue({
            data: {
                followers: [],
                cursor: undefined
            }
        });

        await runOptimizedBatch(mockBot, mockLabeler, mockDb);

        // Verify: processUser not called (no followers)
        expect(labeling.processUser).not.toHaveBeenCalled();
        // Verify: negateUser called for the leaver
        expect(labeling.negateUser).toHaveBeenCalledWith('did:user:leave', mockLabeler, mockDb);
    });

    it('DBにない新規フォロワーを処理する', async () => {
        // Setup DB: Empty
        mockDb.prepare.mockReturnValue({
            all: () => [],
        });

        // Setup Bot: New follower found
        mockBot.agent.get.mockResolvedValue({
            data: {
                followers: [{ did: 'did:user:new', handle: 'handle.new' }],
                cursor: undefined
            }
        });

        await runOptimizedBatch(mockBot, mockLabeler, mockDb);

        // Verify: processUser called for new follower
        expect(labeling.processUser).toHaveBeenCalledWith('did:user:new', mockLabeler, 'handle.new');
        expect(labeling.negateUser).not.toHaveBeenCalled();
    });

    it('ページネーションを正しく処理する', async () => {
        // Setup DB: User from page 2 is in DB
        mockDb.prepare.mockReturnValue({
            all: () => [{ uri: 'did:user:page2' }],
        });

        // Setup Bot: Two pages of followers
        mockBot.agent.get
            .mockResolvedValueOnce({
                data: {
                    followers: [{ did: 'did:user:page1', handle: 'handle.page1' }],
                    cursor: 'next-page-cursor'
                }
            })
            .mockResolvedValueOnce({
                data: {
                    followers: [{ did: 'did:user:page2', handle: 'handle.page2' }],
                    cursor: undefined // End of list
                }
            });

        await runOptimizedBatch(mockBot, mockLabeler, mockDb);

        // Verify: Both pages processed
        expect(labeling.processUser).toHaveBeenCalledWith('did:user:page1', mockLabeler, 'handle.page1');
        expect(labeling.processUser).toHaveBeenCalledWith('did:user:page2', mockLabeler, 'handle.page2');

        // Verify: Page 2 user NOT deleted (crucial check)
        expect(labeling.negateUser).not.toHaveBeenCalled();
    });

    it('APIエラー時に処理を中断し、誤削除を防ぐ', async () => {
        // Setup DB: Users exist
        mockDb.prepare.mockReturnValue({
            all: () => [{ uri: 'did:user:existing' }],
        });

        // Setup Bot: API fails
        mockBot.agent.get.mockRejectedValue(new Error('API Error'));

        // Expect runOptimizedBatch to reject/throw
        await expect(runOptimizedBatch(mockBot, mockLabeler, mockDb)).rejects.toThrow('API Error');

        // Verify: negateUser MUST NOT be called
        expect(labeling.negateUser).not.toHaveBeenCalled();
    });
});

describe('startMidnightScheduler', () => {
    let mockBot: any;
    let mockLabeler: any;
    let mockDb: any;

    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();

        mockBot = {
            profile: { did: 'did:bot' },
            agent: { get: vi.fn().mockResolvedValue({ data: { followers: [] } }) },
        } as unknown as Bot;

        mockLabeler = {} as unknown as LabelerServer;

        // Mock DB to spy on prepare (which runOptimizedBatch calls)
        mockDb = {
            prepare: vi.fn().mockReturnValue({ all: () => [] })
        };
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    it('起動時に即座にバッチを実行する', () => {
        (utils.getJstDate as any).mockReturnValue('2024-01-01');

        startMidnightScheduler(mockBot, mockLabeler, mockDb);

        // Check if DB was accessed immediately (async call started)
        expect(mockDb.prepare).toHaveBeenCalled();
    });

    it('日付が変わった時にバッチを実行する', async () => {
        // Initial State: 2024-01-01
        (utils.getJstDate as any).mockReturnValue('2024-01-01');
        startMidnightScheduler(mockBot, mockLabeler, mockDb);

        // Reset mocks to ignore the initial run
        mockDb.prepare.mockClear();

        // Advance 1 minute, Date same -> No action
        await vi.advanceTimersByTimeAsync(60000);
        expect(mockDb.prepare).not.toHaveBeenCalled();

        // Advance 1 minute, Date changed to 2024-01-02 -> Action triggers
        (utils.getJstDate as any).mockReturnValue('2024-01-02');
        await vi.advanceTimersByTimeAsync(60000);

        expect(mockDb.prepare).toHaveBeenCalled();
    });
});
