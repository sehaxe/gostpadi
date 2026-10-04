// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  // Куда ляжет собранное: /gostpadi/docs/ на GitHub Pages.
  // Репозиторий sehaxe/gostpadi — это project page, поэтому перед docs
  // стоит ещё и имя репозитория. Без base ссылки на ассеты и страницы
  // ушли бы в корень домена, то есть на sehaxe.github.io/_astro/…
  // мимо репозитория.
  base: '/gostpadi/docs',
  site: 'https://sehaxe.github.io',
  trailingSlash: 'ignore',
  build: { format: 'directory' },
  integrations: [
    starlight({
      title: 'gostpadi',
      description: 'Блок-схемы по ГОСТ 19.701 из C-кода',
      logo: { light: './src/assets/logo.svg', dark: './src/assets/logo.svg' },
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/sehaxe/gostpadi',
        },
      ],
      // index.md — посадочная страница, Starlight выносит её в / и
        // в боковое меню не добавляет; дублировать ссылку не надо
      sidebar: [
        {
          label: 'Пользование',
          items: [
            { label: 'Установка и запуск', slug: 'cli' },
            { label: 'Что понимается в коде', slug: 'supported' },
          ],
        },
        {
          label: 'Разработка',
          items: [
            { label: 'Как устроен движок', slug: 'architecture' },
            { label: 'Устойчивость к ошибкам', slug: 'robustness' },
          ],
        },
      ],
    }),
  ],
});
