# Celestial Mono Concept

Автономный HTML-прототип нового чёрно-белого небесного интерфейса.

Открыть: `newdesign2/index.html`.

Прототип теперь построен от реальной структуры приложения:

- app shell из `src/pages/_layout.tsx`: titlebar, sidebar, nav collapse, traffic block, content area;
- разделы из `src/pages/_routers.tsx`: Home, Proxies, Profiles, Subscription Management, Connections, Rules, Logs, Notifications, Settings;
- главный экран по `src/pages/home.tsx` и `components/home/*`: профиль, текущий узел, сеть, режим, трафик;
- прокси по `src/pages/proxies.tsx` и `components/proxy/*`: provider controls, rule/global/direct, chain mode, virtual group list;
- профили, соединения, правила, логи и настройки отражают реальные рабочие поверхности.

Визуальное направление: плотный desktop UI, монохромная палитра, тонкая небесная графика, без цветных акцентов и без лендинговой подачи.
