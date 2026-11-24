# Changelog

## [0.5.0](https://github.com/xiprox/plates/compare/v0.4.0...v0.5.0) (2026-11-24)


### Features

* connect Plates to Claude Code from its menus ([bcf4f34](https://github.com/xiprox/plates/commit/bcf4f344c56ad45b04ca491c09b4a06d856dce9e))


### Bug Fixes

* stop the rack showing through a menu on macOS ([589d473](https://github.com/xiprox/plates/commit/589d473a6dbf4f72420e5fcb8e2b91d0ff62c62f))

## [0.4.0](https://github.com/xiprox/plates/compare/v0.3.0...v0.4.0) (2026-11-23)


### Features

* clear a session's task list when its tile goes, and name what an agent tile runs ([184e6fc](https://github.com/xiprox/plates/commit/184e6fcc172bd412dba81175118c78b84bf52b72))
* dismissing a session takes its sub-sessions along ([7e92ca0](https://github.com/xiprox/plates/commit/7e92ca06fda1df11eae1bac34c814f3d532eca39))
* draw a message's tables as tables on the hover card ([58ca544](https://github.com/xiprox/plates/commit/58ca544ea635cd1106744fcb7ad6b05de2a5fdd9))
* forget a single tile from the archive ([b96c8e6](https://github.com/xiprox/plates/commit/b96c8e6ebfc990202eb3dbf3107074ef0eb30bfb))
* hold a timer's tone and a tile's call for attention while you type ([55e5fe0](https://github.com/xiprox/plates/commit/55e5fe0ec0d00e2fcdc91f811208d3c832d91a14))
* hold pings while you read a reply in the Claude app ([3c001dd](https://github.com/xiprox/plates/commit/3c001ddab61913e04186b64f368bc96cc5547db0))
* let a session wait on CI and ping you when it's done ([a2b63e9](https://github.com/xiprox/plates/commit/a2b63e9665cf109e03f5589c6dcddb212c97d645))
* make the timer's tone a soft felt-mallet strike on a glass bar ([19d424c](https://github.com/xiprox/plates/commit/19d424c1f8ccb43a2ffb5a317f714a4232a128c8))
* nest sub-sessions two levels deep, with unbroken lines ([96d56dc](https://github.com/xiprox/plates/commit/96d56dc4bd30f2873f1b21ff48d0842d7b9e4639))
* register the task server when you install Plates' hooks ([ee2975e](https://github.com/xiprox/plates/commit/ee2975ee845508ba82adf9b482633187cbeb0e17))
* ring a soft tone when a timer runs out ([4c26c3f](https://github.com/xiprox/plates/commit/4c26c3fa759ae3118155981637cd5e4c5f458001))
* say "All done" on a task list with nothing left open ([21bfb17](https://github.com/xiprox/plates/commit/21bfb171d26404ed6c765f5922a005f4a44d0984))
* say "Archived" when a tile you take off the rack goes to the archive ([3f6157b](https://github.com/xiprox/plates/commit/3f6157b1fc389cdcd0d28e2717220847416553ba))
* show a working session's plan on its hover card ([511d461](https://github.com/xiprox/plates/commit/511d4616dc42f6ea237801c0607206a39a918c10))
* show each session's task list in its tile ([a58f41a](https://github.com/xiprox/plates/commit/a58f41a0114b93ea0962987792c98c1766fba8bc))


### Bug Fixes

* have Claude mark a turn waiting when it waits on a background command ([2117341](https://github.com/xiprox/plates/commit/2117341d65a5775eb4f87a85ab40bdb2baa0dd5b))
* have Claude update a task as soon as your answer moves it ([a20d9cf](https://github.com/xiprox/plates/commit/a20d9cf8327c837adeb4e7e8b101341d63401afb))
* keep a desktop session's tile when the app moves or detaches it ([84a5916](https://github.com/xiprox/plates/commit/84a59160dd758bc419ededa09968970dc364b4f2))
* keep the rack's frames on time while you drag or throw it on macOS ([a520418](https://github.com/xiprox/plates/commit/a5204187f165c612837f8494a6c6547734c7c29c))
* line the moon up with the count of snoozed tiles beside it ([e92cd3d](https://github.com/xiprox/plates/commit/e92cd3d626d38b8370cf5bc6a4587096edca1d7a))
* show a session's task list on Windows ([453e805](https://github.com/xiprox/plates/commit/453e805445f67b02d12080067ede52debb7d28b0))
* stop a background compaction leaving a tile stuck on "Compacting session…" ([385819c](https://github.com/xiprox/plates/commit/385819cce28cb2877b247f52ca212a4827efa9f3))
* stop reminding you of a session you've already opened ([a5a4eeb](https://github.com/xiprox/plates/commit/a5a4eeb1bb0b2a0cc0421908554abf8d5edc8745))
* stop the rack's tiles wobbling as it resizes on macOS ([b269bc9](https://github.com/xiprox/plates/commit/b269bc9ea643b65810eaef7d84d068a8d5aa2583))

## [0.3.0](https://github.com/xiprox/plates/compare/v0.2.0...v0.3.0) (2026-11-16)


### Features

* keep the tiles you remove in an archive you can bring them back from ([e91c597](https://github.com/xiprox/plates/commit/e91c597958ac5ccb4684ab6ae0ff2da413d7cc25))


### Bug Fixes

* stop the docked strip flickering when you hover it on macOS ([5a60544](https://github.com/xiprox/plates/commit/5a60544076e3724059e77ebd822d9892f5ca3a71))

## [0.2.0](https://github.com/xiprox/plates/compare/v0.1.0...v0.2.0) (2026-11-16)


### Features

* fill a due reminder's hourglass with yellow ([6a16159](https://github.com/xiprox/plates/commit/6a16159e53d305dd4a9f7aeb4b8a4dbb7590438c))
* put Plates in the tray and menu bar, with its own icon, and fix it on macOS ([6166d31](https://github.com/xiprox/plates/commit/6166d31935a9049a7b5a072aff23fd314322d475))
* snooze tiles, swipe them away, and answer wrap-ups on macOS ([03b215f](https://github.com/xiprox/plates/commit/03b215f8be37afae03a9134b78b8984d4a1340a6))

## 0.1.0 (2026-11-15)


### Features

* release Plates for Windows and macOS with Velopack ([ca4c734](https://github.com/xiprox/plates/commit/ca4c73461c43c193e826cb46b296f9d925da56aa))
* remind you of a tile tomorrow, mark turns unread, and start at login ([2b3ee3e](https://github.com/xiprox/plates/commit/2b3ee3e17933405efc2b674f789778af9b9fc36d))
