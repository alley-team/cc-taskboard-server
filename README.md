# cc-taskboard-server

Сервер для работы приложения CC TaskBoard, предназначенного для отслеживания времени выполнения задач и составления графика работы на сутки и более.

## Компиляция и запуск

### Docker Compose

Сперва заполните `env.example` и сохраните как `.env`.

Для сборки введите команду:

```bash
docker compose build
```

Для запуска введите команду:

```bash
docker compose up -d
```

## API

Описания методов REST API находятся в файле [API.md](./API.md).

## Лицензия

Исходный код сервера опубликован по лицензии GNU General Public License третьей версии ([см. текст](./LICENSE)).
