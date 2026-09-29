import { describe, expect, it } from 'vitest';
import { codeEditorAttributes } from './codeEditor';

describe('codeEditorAttributes', () => {
  it('names the editor by its visible label and describes it', () => {
    expect(
      codeEditorAttributes({
        ariaLabel: 'Lua script',
        ariaLabelledBy: 'row-label',
        ariaDescribedBy: 'row-desc',
      }),
    ).toEqual({ 'aria-labelledby': 'row-label', 'aria-describedby': 'row-desc' });
  });

  it('falls back to a plain name', () => {
    expect(codeEditorAttributes({ ariaLabel: 'send template' })).toEqual({
      'aria-label': 'send template',
    });
  });

  it('adds nothing it was not given', () => {
    expect(codeEditorAttributes({})).toEqual({});
  });
});
