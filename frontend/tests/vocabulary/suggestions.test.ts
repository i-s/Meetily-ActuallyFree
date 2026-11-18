import { expect, test } from 'bun:test';
import { suggestVocabulary } from '../../src/lib/vocabulary-suggestions';
const rows = (text: string) => [{id:'line', text, audio_start_time:12.5}];
test('suggests exact RU/EN acronym, internal CamelCase and letter+digit source spellings', () => {
  const result = suggestVocabulary(rows('Обсудили СУБД, OpenAPI и M4. обычные слова Hello Привет 2026.'), new Set());
  expect(result.candidates.map(c => c.term)).toEqual(['СУБД','OpenAPI','M4']);
  expect(result.candidates[0]).toMatchObject({transcriptId:'line', audioStartTime:12.5});
  expect(result.candidates[0].excerpt).toContain('Обсудили СУБД, OpenAPI и M4.');
});
test('Cyrillic token forms, compounds, case dedup and existing/suppressed terms retain first exact spelling', () => {
  const result = suggestVocabulary(rows('СУБД Субд ЁлкаAPI Ёлка2 API API openAPI OpenAPI GPT-4'), new Set(['субд','api']));
  expect(result.candidates.map(c=>c.term)).toEqual(['ЁлкаAPI','Ёлка2','openAPI','GPT-4']);
});
test('never fabricates candidates from punctuation, controls, long terms or ordinary title case', () => {
  expect(suggestVocabulary(rows('Я I a 12 привет Name Имя '+ 'A'.repeat(65)),new Set()).candidates).toEqual([]);
  expect(suggestVocabulary([],new Set())).toEqual({candidates:[],truncated:false});
});
test('limits result and exact source excerpts without crossing line identity', () => {
  const input=[{id:'first',text:'x '.repeat(200)+'OpenAPI '+'y '.repeat(200)},{id:'second',text:'API СУБД M4 Ёлка2 GPT-4 ExtraAPI'}];
  const result=suggestVocabulary(input,new Set());
  expect(result.candidates).toHaveLength(5); expect(result.truncated).toBe(true);
  expect(result.candidates[0].excerpt.length).toBeLessThanOrEqual(164);
  expect(result.candidates[0].excerpt).toContain('OpenAPI'); expect(result.candidates[0].transcriptId).toBe('first');
});
test('bounds characters and rows and does not promote a partial token at truncation', () => {
  const result=suggestVocabulary(rows(' '.repeat(99997)+'APIExtra'),new Set());
  expect(result.candidates).toEqual([]); expect(result.truncated).toBe(true);
  const many=Array.from({length:2001},(_,i)=>({id:String(i),text:i===2000?'HiddenAPI':'ordinary'}));
  expect(suggestVocabulary(many,new Set())).toEqual({candidates:[],truncated:true});
});
test('excluded terms do not consume five slots and analysis leaves inputs untouched', () => {
  const input=rows('API СУБД M4 Ёлка2 OpenAPI ExtraAPI AnotherAPI');
  const before=JSON.stringify(input); const excluded=new Set(['api','субд']);
  expect(suggestVocabulary(input,excluded).candidates.map(c=>c.term)).toEqual(['M4','Ёлка2','OpenAPI','ExtraAPI','AnotherAPI']);
  expect(JSON.stringify(input)).toBe(before); expect([...excluded]).toEqual(['api','субд']);
});
test('UTF-16 clipping never offers a prefix before or inside a supplementary Unicode letter', () => {
  for (const spaces of [99996,99997]) {
    const result=suggestVocabulary(rows(' '.repeat(spaces)+'API𐐀Extra'),new Set());
    expect(result.candidates).toEqual([]); expect(result.truncated).toBe(true);
  }
});
test.each(['ёлкаAPI', 'ËlkaAPI', 'OpenAPÍClient'])('combining marks preserve whole exact source spelling: %s', (term) => {
  const result=suggestVocabulary(rows(`Обсудили ${term} и ControlAPI.`),new Set());
  expect(result.candidates.map(c=>c.term)).toEqual([term,'ControlAPI']);
  expect(result.candidates[0].excerpt).toContain(term);
});
test('marks before, at and beyond the UTF-16 budget never expose token fragments', () => {
  for (const term of ['ёлкаAPI','ËlkaAPI','OpenAPÍClient','OpenAPI\u{1D165}Client']) {
    const mark=term.search(/\p{M}/u);
    for (const offset of [0,1,2]) {
      const result=suggestVocabulary(rows(' '.repeat(100000-mark-offset)+term),new Set());
      expect(result.candidates).toEqual([]);expect(result.truncated).toBe(true);
    }
  }
});
test('all Unicode mark categories continue a token, including a leading unsupported mark', () => {
  for (const mark of ['́','ः','⃝','\u{1D165}']) {
    const term=`OpenAPI${mark}Client`;
    expect(suggestVocabulary(rows(term),new Set()).candidates.map(c=>c.term)).toEqual([term]);
    expect(suggestVocabulary(rows(`${mark}OpenAPI`),new Set()).candidates).toEqual([]);
  }
});
