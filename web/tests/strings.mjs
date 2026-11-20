// The editor's counted strings in every locale it ships: node tests/strings.mjs. The status bar's
// summary names its elements and pages in the locale's plural form ("1 element · 1 page", "2
// elements · 3 pages"), never "1 pages". Strings are looked up as the kit editor's createKitStrings
// does (the locale, its language, en-US, then the key itself), with `{name}` values filled in.
import { CATALOGS, counted, summary } from '../src/strings.ts'

let failures = 0
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
}

const stringsFor = (locale) => {
  const chain = [CATALOGS[locale], CATALOGS[locale.split('-')[0]], CATALOGS['en-US']].filter(Boolean)
  return (key, values = {}) => {
    const found = chain.find((catalog) => Object.hasOwn(catalog, key))
    const text = found ? found[key] : key
    return text.replace(/\{(\w+)\}/g, (whole, name) => (name in values ? String(values[name]) : whole))
  }
}

const en = stringsFor('en-US')
for (const [elements, pages, expected] of [
  [0, 1, '0 elements · 1 page'],
  [1, 1, '1 element · 1 page'],
  [10, 1, '10 elements · 1 page'],
  [1, 2, '1 element · 2 pages'],
  [21, 3, '21 elements · 3 pages'],
]) {
  const said = summary(en, 'en-US', elements, pages)
  step(`en-US: ${expected}`, said === expected, said)
}

for (const locale of Object.keys(CATALOGS)) {
  const t = stringsFor(locale)
  const rules = new Intl.PluralRules(locale)
  for (const count of [0, 1, 2, 3, 5, 11, 21, 101]) {
    const said = summary(t, locale, count, count)
    step(`${locale}: ${count} and ${count} fill every value`, !/[{}]/.test(said) && said.split(String(count)).length === 3, said)
    // Each count reads the locale's own form for its plural category, or the `other` form.
    for (const key of ['elementCount', 'pageCount']) {
      const category = rules.select(count)
      const own = CATALOGS[locale][`${key}.${category}`] ?? CATALOGS[locale][`${key}.other`]
      step(`${locale}: ${key} ${count} is the locale's ${category} form`, own !== undefined && counted(t, locale, key, count) === own.replace('{count}', String(count)), counted(t, locale, key, count))
    }
  }
  step(`${locale}: has every plural category it uses`, ['elementCount', 'pageCount'].every((key) => Object.hasOwn(CATALOGS[locale], `${key}.other`)))
}

console.log(failures ? `strings: ${failures} failed` : 'strings: all passed')
process.exit(failures ? 1 : 0)
