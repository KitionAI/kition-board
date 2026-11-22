/**
 * The board's clipboard (design note 4.3): copy, cut and paste on the engine's `clipboard.copy`
 * and `element.paste`, and pictures pasted or dropped on the canvas (`image.insert` with `data` and
 * `mime`, the picture tool's path).
 *
 * The payload is the engine's (`application/x-kition-board+json`); the editor never builds one. A
 * copy keeps it here and also writes it to the system clipboard as text prefixed `kition-board:`
 * when the browser lets it (an embedded frame or headless WebKit may refuse). A paste takes a
 * `kition-board:` text from the system clipboard when the last copy here reached it (or there was
 * none here), else the copy kept here. Repeated pastes at one place step 16 units each; a new copy
 * starts again.
 */
import type { Point } from './geometry'

export const PAYLOAD_TYPE = 'application/x-kition-board+json'
/** What a board payload on the system clipboard starts with. */
const PREFIX = 'kition-board:'
/** How far each repeated paste at the same place goes from the one before. */
export const PASTE_STEP = 16
/** How far each further picture of one paste or drop goes from the one before. */
export const PICTURE_STEP = 24
/** The longest side of a picture placed at its natural size, in board units. */
export const MAX_PICTURE_SIDE = 800
/** How long the editor waits for the system clipboard (or a picture's decoding) before going on without it. */
const WAIT_MS = 500

/** The picture types the engine stores, by file extension (`PICTURE_EXTENSIONS` in tools.ts). */
export const PICTURE_MIME: Readonly<Record<string, string>> = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', svg: 'image/svg+xml' }
const PICTURE_TYPES = new Set(Object.values(PICTURE_MIME))

export type Payload = { type: string; version: number; elements: unknown[] }

/** The last copy made here. */
let held: Payload | null = null
/** Whether the last copy also reached the system clipboard. */
let reachedSystem = false
/** The copy under way, which a paste waits for. */
let copying: Promise<void> | null = null
/** The last paste's place and how many pastes in a row went there. */
let repeat: { at: Point; count: number } | null = null

function isPayload(value: unknown): value is Payload {
  const payload = value as Payload | null
  return payload != null && typeof payload === 'object' && payload.type === PAYLOAD_TYPE && Array.isArray(payload.elements) && payload.elements.length > 0
}

function parse(text: string | null): Payload | null {
  if (!text?.startsWith(PREFIX)) return null
  try {
    const value: unknown = JSON.parse(text.slice(PREFIX.length))
    return isPayload(value) ? value : null
  } catch {
    return null
  }
}

/** `promise`'s value, or null when it fails or takes longer than `ms`. */
function within<T>(promise: Promise<T>, ms: number): Promise<T | null> {
  return Promise.race([promise.catch(() => null), new Promise<null>((resolve) => window.setTimeout(() => resolve(null), ms))])
}

/** A reading engine command (`mutates: false`), answered as parsed JSON or null. */
export type Query = (command: string, params: Record<string, unknown>) => Promise<unknown>

/** Copies `ids` and everything inside them; answers whether the engine gave a payload. */
export function copy(query: Query, ids: readonly string[]): Promise<boolean> {
  const run = (async () => {
    const payload = await query('clipboard.copy', { ids: [...ids] })
    if (!isPayload(payload)) return false
    held = payload
    repeat = null
    reachedSystem = false
    const clipboard = typeof navigator === 'undefined' ? undefined : navigator.clipboard
    if (clipboard?.writeText) reachedSystem = (await within(clipboard.writeText(PREFIX + JSON.stringify(payload)).then(() => true), WAIT_MS)) === true
    return true
  })().catch(() => false)
  const settled = run.then(() => undefined)
  copying = settled
  void settled.then(() => {
    if (copying === settled) copying = null
  })
  return run
}

/** Whether a paste has a copy to place without asking the system clipboard. */
export function holding(): boolean {
  return held != null
}

/** What a paste event, a drop or the system clipboard carries, read while the event lasts (its data empties afterwards). */
export type Carried = { files: File[]; text: string | null; stale: boolean }

/** The picture type of `file` (its own type, else its extension's), or null when it is no picture the engine stores. */
export function pictureMime(file: { type: string; name: string }): string | null {
  if (PICTURE_TYPES.has(file.type)) return file.type
  const extension = file.name.slice(file.name.lastIndexOf('.') + 1).toLowerCase()
  return PICTURE_MIME[extension] ?? null
}

/** The pictures and the text of a paste's `clipboardData` or a drop's `dataTransfer`. */
export function carried(data: DataTransfer | null | undefined): Carried {
  const files: File[] = []
  let text: string | null = null
  if (data) {
    try {
      files.push(...Array.from(data.files ?? []))
      // Some browsers list a pasted picture only among the items.
      if (!files.length)
        for (const item of Array.from(data.items ?? [])) {
          const file = item.kind === 'file' ? item.getAsFile() : null
          if (file) files.push(file)
        }
      text = data.getData('text/plain') || null
    } catch {
      // A browser that does not let the page read it: nothing carried.
    }
  }
  // A text read while a copy is under way is older than that copy.
  return { files: files.filter((file) => pictureMime(file) != null), text, stale: copying != null }
}

/** The system clipboard's text, when the browser lets the editor read it in time and it could matter. */
export async function systemText(): Promise<Carried> {
  const stale = copying != null
  const clipboard = typeof navigator === 'undefined' ? undefined : navigator.clipboard
  const text = clipboard?.readText && (reachedSystem || !held) ? await within(clipboard.readText(), WAIT_MS) : null
  return { files: [], text, stale }
}

/** The board payload a paste places: a board text on the system clipboard when it can be newer than the copy here, else that copy. */
export async function payloadFor(from: Carried | null): Promise<Payload | null> {
  if (copying) await copying
  const fromText = from && !from.stale ? parse(from.text) : null
  if (fromText && (reachedSystem || !held)) return fromText
  return held
}

/** Where a paste at `at` puts the copies' top-left: one step further for each paste in a row at the same place. */
export function pastePlace(at: Point): Point {
  const again = repeat != null && Math.abs(repeat.at.x - at.x) < 0.5 && Math.abs(repeat.at.y - at.y) < 0.5
  const count = again && repeat ? repeat.count + 1 : 0
  repeat = { at, count }
  return { x: at.x + count * PASTE_STEP, y: at.y + count * PASTE_STEP }
}

export function base64(bytes: Uint8Array): string {
  let out = ''
  for (let at = 0; at < bytes.length; at += 0x8000) out += String.fromCharCode(...bytes.subarray(at, at + 0x8000))
  return btoa(out)
}

/** A picture's size in pixels, as the browser decodes it; null when it cannot. */
async function naturalSize(bytes: Uint8Array, mime: string): Promise<{ w: number; h: number } | null> {
  const url = URL.createObjectURL(new Blob([bytes.slice().buffer as ArrayBuffer], { type: mime }))
  try {
    const image = new Image()
    image.src = url
    const decoded = await within(image.decode().then(() => true), WAIT_MS * 4)
    return decoded && image.naturalWidth > 0 && image.naturalHeight > 0 ? { w: image.naturalWidth, h: image.naturalHeight } : null
  } finally {
    URL.revokeObjectURL(url)
  }
}

/**
 * `image.insert`'s parameters for a picture's bytes (the picture tool, a paste and a drop): centred
 * on `centre` at its natural size, at most 800 units on its longer side (320 × 240 when the browser
 * cannot read its size). The engine checks the bytes, their type and its size limit.
 */
export async function pictureParams(bytes: Uint8Array, mime: string, alt: string, centre: Point | null): Promise<Record<string, unknown>> {
  const natural = await naturalSize(bytes, mime)
  const scale = natural ? Math.min(1, MAX_PICTURE_SIDE / Math.max(natural.w, natural.h)) : 1
  const size = natural ? [natural.w * scale, natural.h * scale] : [320, 240]
  const params: Record<string, unknown> = { data: base64(bytes), mime, size }
  if (alt) params.alt = alt
  if (centre) params.position = [centre.x - size[0] / 2, centre.y - size[1] / 2]
  return params
}
