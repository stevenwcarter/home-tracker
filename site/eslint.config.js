import js from '@eslint/js';
import globals from 'globals';
import reactHooks from 'eslint-plugin-react-hooks';
import jsxA11y from 'eslint-plugin-jsx-a11y';
import reactRefresh from 'eslint-plugin-react-refresh';
import tseslint from 'typescript-eslint';
import eslintConfigPrettier from 'eslint-config-prettier';
import eslintPluginPrettierRecommended from 'eslint-plugin-prettier/recommended';

export default tseslint.config(
  {
    ignores: ['dist/**', 'build/**', 'node_modules/**', '.vite/**'],
  },
  eslintConfigPrettier,
  eslintPluginPrettierRecommended,
  jsxA11y.flatConfigs.recommended,
  {
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    files: ['**/*.{ts,tsx}'],
    // ignores: [
    //   "src/stories/**",
    //   "node_modules/**",
    //   "build/**",
    //   "storybook-static/**",
    // ],
    languageOptions: {
      parserOptions: {
        ecmaFeatures: {
          jsx: true,
        },
      },
      ecmaVersion: 2023,
      globals: globals.browser,
    },
    plugins: {
      jsxA11y,
      'react-hooks': reactHooks,
      'react-refresh': reactRefresh,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      'react-refresh/only-export-components': ['warn', { allowConstantExport: true }],
      'eslint-comments/no-unused-disable': 0,
      'prettier/prettier': [
        'error',
        {
          singleQuote: true,
          printWidth: 100,
        },
      ],
      // 'react/jsx-uses-react': 'error',
      // 'react/jsx-uses-vars': 'error',
      'no-confusing-arrow': 0,
      'no-console': [
        'error',
        {
          allow: ['log', 'warn', 'error'],
        },
      ],
      'no-shadow': 0,
      'no-template-curly-in-string': 0,
      'no-unused-vars': 0,
      'no-use-before-define': 0,
      'sort-imports': 0,
      'react-hooks/set-state-in-effect': 0,
      '@typescript-eslint/explicit-function-return-type': 0,
      '@typescript-eslint/explicit-module-boundary-types': 0,
      '@typescript-eslint/no-empty-function': 0,
      '@typescript-eslint/no-empty-interface': 0,
      '@typescript-eslint/no-empty-object-type': 0,
      '@typescript-eslint/no-explicit-any': 0,
      '@typescript-eslint/no-non-null-assertion': 0,
      '@typescript-eslint/no-shadow': 1,
      'jsx-a11y/no-noninteractive-element-interactions': 0,
      'jsx-a11y/no-static-element-interactions': 0,
      'jsx-a11y/click-events-have-key-events': 0,
      'jsx-a11y/label-has-associated-control': [
        2,
        {
          // "labelComponents": ["CustomInputLabel"],
          // "labelAttributes": ["label"],
          controlComponents: ['Input'],
          depth: 3,
        },
      ],
      '@typescript-eslint/no-unused-vars': [
        'error',
        {
          args: 'after-used',
          argsIgnorePattern: '([aA]ction|^_)',
          caughtErrorsIgnorePattern: '^_',
          destructuredArrayIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          caughtErrors: 'none',
          ignoreRestSiblings: true,
        },
      ],
      '@typescript-eslint/no-var-requires': 0,
    },
  },
);
