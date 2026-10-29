import { describe, expect, it } from 'vitest'
import { extractImageReferences, markdownImageLink } from './markdown'

describe('Markdown image references', () => {
  it('finds relative image links outside fenced code and decodes spaces', () => {
    const markdown = '![hero](<../images/hero%20art.png>)\n\n```md\n![not-real](skip.png)\n```\n![other](images/other.jpg)'
    expect(extractImageReferences(markdown)).toEqual(['../images/hero art.png', 'images/other.jpg'])
  })

  it('builds a relative image link from a nested document', () => {
    expect(markdownImageLink('DesignDocs/02/flow.md', 'DesignDocs/images/flow chart.svg'))
      .toBe('![flow chart](<../images/flow chart.svg>)')
  })
})
