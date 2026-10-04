// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  // Документация живёт по пути /docs/ рядом с приложением в корне сайта,
  // поэтому base не нужен: собирается в site/dist и копируется в docs/docs.
  site: 'https://sehaxe.github.io',
  trailingSlash: 'ignore',
  build: { format: 'directory' },
  integrations: [
    starlight({
      title: 'gostpadi',
      description: 'Блок-схемы по ГОСТ 19.701 из C-кода',
      logo: { light: './src/assets/logo.svg', dark: './src/assets/logo.svg' },
      favicon: '/og.png',
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
