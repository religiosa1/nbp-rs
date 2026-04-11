# Personal helper service to retrieve Exchange Rates from Narodowy Bank Polski

A small service, that retrieves currency exchange rates from Narodowy Bank Polski
and displays the base exchange rates, as well as USD -> EUR conversion rate from
Tabela A of average exchange rates.

## How it works

It retrieves the RSS feed exposed by NBP on the url https://rss.nbp.pl/kursy/TabelaA.xml,
parses its items as well as embedded html description summary, that contains
conversion rates.

Depending on `Accept` request header it either displays the data in JSON (the default)
or a webpage, if `Accept: text/html` is passed.

## Configuration

All of the configuration is performed through the environment variables.

- `NBP_URL` defaults to `https://rss.nbp.pl/kursy/TabelaA.xml`, address of the RSS feed to fetch;

The intended service usage is through the systemd unix socket, so service listens
on the unix socket provided as FD#3. For local development it listens on the TCP
sockets directly, using the following env vars for configuration:

- `NBP_PORT` TCP port to bind to, defaults to 3000;
- `NBP_ADDR` TCP address to bind to, defaults to `127.0.0.1`;

## Deployment

This service is intended to be deployed as a systemd service behind a systemd
socket and a nginx reverse proxy on a VPS. So it runs only when there's
a request to it. As I really need it only once per month, when I do my invoices,
no point in it constantly running.

The required systemd unit files are placed in the [deploy](./deploy) folder of
the repo.

## License

nbp_rs is MIT licensed
