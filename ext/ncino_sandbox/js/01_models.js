
export class RequestModel {
  cache;
  credentials;
  method;
  destination;
  headers;
  integrity;
  mode;
  redirect;
  referrer;
  referrerPolicy;
  keepalive;
  priority;
  url;
  constructor(req) {
    this.cache = req.cache;
    this.credentials = req.credentials;
    this.method = req.method;
    this.destination = req.destination;
    this.headers = req.headers;
    this.integrity = req.integrity;
    this.mode = req.mode;
    this.redirect = req.redirect;
    this.referrer = req.referrer;
    this.referrerPolicy = req.referrerPolicy;
    this.keepalive = req.keepalive;
    this.priority = req.priority;
    // cannot clone abort signal since it can't be serialized.
    this.url = req.url;
  }

  toRequest(body) {
    const r = new Request(this.url, {
      method: this.method,
      headers: this.headers,
      body,
      mode: this.mode,
      credentials: this.credentials,
      cache: this.cache,
      redirect: this.redirect,
      referrer: this.referrer,
      integrity: this.integrity,
      keepalive: this.keepalive,
      priority: this.priority,
      destination: this.destination,
      referrerPolicy: this.referrerPolicy,
    });

    return r;
  }
}

export class ResponseModel {
  status;
  statusText;
  headers;

  constructor(res) {
    this.status = res.status;
    this.statusText = res.statusText;
    this.headers = res.headers;
  }

  toResponse(body) {
    const r = new Response(body, {
      status: this.status,
      statusText: this.statusText,
      headers: this.headers,
    });

    return r;
  }
}
