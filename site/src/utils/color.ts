const HEX_COLOR = /^#(?:[0-9a-f]{3}|[0-9a-f]{6})$/i;

/**
 * Whether `value` is a `#rgb` or `#rrggbb` colour. The server checks colours
 * it is given, but imported tags can carry anything, so a swatch is drawn only
 * for a value that passes this.
 */
export const isHexColor = (value: string | null | undefined): value is string =>
  typeof value === 'string' && HEX_COLOR.test(value);
