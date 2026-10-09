// The first module every window runs. It paints the theme the last
// window left in the cache (theme/themePaint) on the document root, so
// the first frame is already in your theme, light or dark, instead of
// the dark stylesheet defaults. While the theme follows the game it
// paints the day or night last shown, until World.Time comes again. main.tsx imports it first, and imports
// run in order, so this lands before any other module and before React
// renders.
import { prepaintTheme } from './theme/themePaint';

prepaintTheme();
