import { describe, expect, it } from 'vitest'
import { defaultPushTarget, maskRemoteUrl, pushActionDisabled, pushConfirmation, pushDisabledReason, pushAssessmentReason, pushWorktreeWarning, remoteBranchChoices, type ManagedBranch } from './gitManagement'

const branches: ManagedBranch[] = [
  { name: 'main', current: true, remote: false, upstream: 'origin/main', ahead: 1, behind: 0 },
  { name: 'feature/ui', current: false, remote: false, upstream: null, ahead: 0, behind: 0 },
  { name: 'origin/HEAD', current: false, remote: true, upstream: null, ahead: 0, behind: 0 },
  { name: 'origin/main', current: false, remote: true, upstream: null, ahead: 0, behind: 0 },
  { name: 'origin/release/1.0', current: false, remote: true, upstream: null, ahead: 0, behind: 0 },
  { name: 'backup/main', current: false, remote: true, upstream: null, ahead: 0, behind: 0 },
]

describe('Git management selections and safeguards', () => {
  it('offers only real branches from the selected remote and excludes its symbolic HEAD', () => {
    expect(remoteBranchChoices(branches, 'origin')).toEqual(['main', 'release/1.0'])
    expect(remoteBranchChoices(branches, 'missing')).toEqual([])
  })

  it('disables Push for incomplete targets while allowing committed content from a dirty worktree', () => {
    const clean = { branch: 'main', changed: [], ahead: 1, behind: 0 }
    expect(pushActionDisabled(clean, 'origin', 'main')).toBe(false)
    expect(pushActionDisabled({ ...clean, changed: ['DesignDocs/page.md'] }, 'origin', 'main')).toBe(false)
    expect(pushActionDisabled(clean, '', 'main')).toBe(true)
    expect(pushActionDisabled(clean, 'origin', '  ')).toBe(true)
  })

  it('shows the branch, remote target, dirty count and divergence in Push confirmation', () => {
    expect(pushConfirmation({ branch: 'feature/ui', changed: ['a.md', 'b.md'], ahead: 2, behind: 1 }, 'origin', 'review'))
      .toContain('feature/ui の既存コミットを origin/review に Push')
    expect(pushConfirmation({ branch: 'feature/ui', changed: ['a.md'], ahead: 2, behind: 1 }, 'origin', 'review'))
      .toContain('未コミット変更 1 件、ahead 2 / behind 1')
  })

  it('masks credentials in remote change confirmations', () => {
    expect(maskRemoteUrl('https://alice:secret@example.invalid/repo?access_token=private&custom-key=private2&view=1'))
      .toBe('https://***@example.invalid/repo?access_token=***&custom-key=***&view=***')
  })
})

describe('daily Push defaults', () => {
  it('keeps Push blocked until the selected target is verified and explains rejected assessments', () => {
    const preview = { current_branch: 'main', remote: 'test', remote_branch: 'first', ready: true, message: '初回 Push' }
    expect(pushAssessmentReason(null, 'main', 'test', 'first')).toContain('未検証')
    expect(pushAssessmentReason(preview, 'main', 'test', 'first')).toBe('')
    expect(pushAssessmentReason(preview, 'main', 'other', 'first')).toContain('やり直し')
    expect(pushAssessmentReason(preview, 'feature', 'test', 'first')).toContain('やり直し')
    for (const message of ['送る新しいコミットがありません', 'リモート履歴が未取得です', '履歴が分岐しています', '認証に失敗しました']) {
      expect(pushAssessmentReason({ ...preview, ready: false, message }, 'main', 'test', 'first')).toBe(message)
    }
  })
  it('explains missing targets, dirty paths and editor protection without requiring upstream', () => {
    const status = { branch: 'main', changed: [], ahead: 0, behind: 0 }
    const options = { busy: false, dirty: false, externalChanged: false, source: 'main' }
    expect(pushDisabledReason(status, '', 'main', options)).toContain('remote')
    expect(pushDisabledReason(status, 'origin', '', options)).toContain('ブランチ')
    expect(pushDisabledReason({ ...status, changed: ['Assets/game.asset'] }, 'origin', 'main', options)).toBe('')
    expect(pushDisabledReason(status, 'origin', 'first', options)).toBe('')
    expect(pushDisabledReason(status, 'origin', 'main', { ...options, dirty: true })).toContain('未保存')
  })
  it('warns that staged and unstaged changes are excluded without blocking a validated Push', () => {
    const status = { branch: 'main', changed: ['Assets/a.asset', 'DesignDocs/draft.md'], ahead: 3, behind: 0 }
    expect(pushWorktreeWarning(status)).toContain('未コミット変更 2 件は送信されません')
    expect(pushConfirmation(status, 'origin', 'main')).toContain('作業ファイルとステージ内容を保持')
    expect(pushAssessmentReason({ current_branch: 'main', remote: 'origin', remote_branch: 'main', ready: true, message: '3 commits' }, 'main', 'origin', 'main')).toBe('')
  })
  it('uses upstream instead of the first remote', () => {
    expect(defaultPushTarget(branches, ['backup', 'origin'])).toEqual({ remote: 'origin', branch: 'main' })
  })
  it('handles first publication and detached HEAD', () => {
    expect(defaultPushTarget([{ ...branches[1], current: true }], ['origin'])).toEqual({ remote: 'origin', branch: 'feature/ui' })
    expect(defaultPushTarget([], [])).toEqual({ remote: '', branch: '' })
  })
  it('handles nested remote and branch names', () => {
    expect(defaultPushTarget([{ ...branches[0], upstream: 'team/origin/release/v2' }], ['team', 'team/origin'])).toEqual({ remote: 'team/origin', branch: 'release/v2' })
  })
})
