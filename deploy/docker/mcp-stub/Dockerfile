FROM python:3.12-alpine

WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY server.py .

EXPOSE 3001

# Run as non-root.
RUN adduser -D -u 10002 mcp && chown mcp /app
USER mcp

CMD ["python", "server.py"]
