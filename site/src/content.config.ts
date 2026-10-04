// Astro 5 требует явного объявления коллекции: без него каталог
// src/content/docs не читается и все slug'и «не существуют».
import { defineCollection } from 'astro:content';
import { docsLoader } from '@astrojs/starlight/loaders';
import { docsSchema } from '@astrojs/starlight/schema';

export const collections = {
  docs: defineCollection({ loader: docsLoader(), schema: docsSchema() }),
};
