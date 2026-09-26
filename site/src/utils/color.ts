const HEX_COLOR = /^#(?:[0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i;

/**
 * Whether `value` is a `#rgb`, `#rrggbb` or `#rrggbbaa` colour. The server checks colours
 * it is given, but imported tags can carry anything, so a swatch is drawn only
 * for a value that passes this. Keep in step with the server's guard,
 * `optional_color` in `src/svc/tag.rs`.
 */
export const isHexColor = (value: string | null | undefined): value is string =>
  typeof value === 'string' && HEX_COLOR.test(value);
