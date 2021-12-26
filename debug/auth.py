#!/usr/bin/env python

import json
import base64

token = input("Введите токен (или оставьте поле пустым, чтобы создать его из логина и пароля): ")

if token != "":
  token = base64.b64encode(bytes(token, 'utf-8'))
else:
  task = {
    "login": input("Введите логин: "),
    "pass": input("Введите пароль: ")
  }
  s = json.dumps(task)
  token = base64.b64encode(bytes(s, 'utf-8'))

print(token)
