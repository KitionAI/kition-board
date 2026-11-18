/** The development host (dev.html): opens `public/sample/Sample.kboard`, or a file chosen in the page. */
import { startDevHost } from '@kition/kit-dev-host'

// The picker answers pictures (the picture tool) and the files cards point at.
startDevHost({ sample: './sample/Sample.kboard', pickable: /\.(png|jpe?g|gif|svg|webp|md|markdown|ktable|pdf|txt)$/i })
