#!/bin/bash

curl -X POST http://localhost:6969/stream \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "What is my Solana public key?",
    "chat_history": [],
    "chain": "solana"
  }'
