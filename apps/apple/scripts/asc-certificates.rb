#!/usr/bin/env ruby
# The signing certificates on the Apple developer account, through the App
# Store Connect API, and the way out of "Your account has reached the
# maximum number of certificates": release.sh archives with automatic
# signing and an API key, and on a runner with an empty keychain Xcode makes
# a new Apple Development certificate for that every time. Its private key
# dies with the runner, so the certificate is of no use to anything
# afterwards, and after a few releases the account has no room for another.
#
#   scripts/asc-certificates.rb            # lists them: type, name, expiry, and which a runner left
#   scripts/asc-certificates.rb --revoke   # revokes the development ones a runner left
#
# "A runner left" is exact: a development certificate whose name says
# "Created via API", which is what Apple calls one Xcode made with a key
# and not a person's. One made on somebody's Mac carries their name and is
# never touched, nor is any distribution certificate.
#
# Needs ASC_KEY_ID, ASC_ISSUER_ID and ASC_KEY_PATH (the .p8), as release.sh
# does. Standard library only. Exits 0 when Apple could not be asked, having
# said so: clearing up is not what a release should fail on.
require "base64"
require "json"
require "net/http"
require "openssl"
require "uri"

API = "https://api.appstoreconnect.apple.com"
DEVELOPMENT = %w[DEVELOPMENT IOS_DEVELOPMENT].freeze
LEFT_BY_A_RUNNER = "Created via API"

def segment(bytes)
  Base64.urlsafe_encode64(bytes, padding: false)
end

# An ES256 token: the signature is r and s, 32 bytes each, not OpenSSL's DER.
def token(key_pem, key_id, issuer, now = Time.now.to_i)
  header = segment(JSON.generate({ alg: "ES256", kid: key_id, typ: "JWT" }))
  claims = segment(JSON.generate({ iss: issuer, iat: now, exp: now + 600, aud: "appstoreconnect-v1" }))
  der = OpenSSL::PKey.read(key_pem).sign(OpenSSL::Digest.new("SHA256"), "#{header}.#{claims}")
  raw = OpenSSL::ASN1.decode(der).value.map { |n| n.value.to_s(2).rjust(32, "\x00".b) }.join
  "#{header}.#{claims}.#{segment(raw)}"
end

def left_by_a_runner?(attributes)
  DEVELOPMENT.include?(attributes["certificateType"]) &&
    [attributes["name"], attributes["displayName"]].compact.any? { |n| n.include?(LEFT_BY_A_RUNNER) }
end

def ask(verb, path, bearer)
  uri = URI("#{API}#{path}")
  request = verb.new(uri)
  request["authorization"] = "Bearer #{bearer}"
  Net::HTTP.start(uri.host, uri.port, use_ssl: true, open_timeout: 15, read_timeout: 30) { |http| http.request(request) }
end

if $PROGRAM_NAME == __FILE__
  revoke = ARGV.include?("--revoke")
  key_id, issuer, key_path = ENV.values_at("ASC_KEY_ID", "ASC_ISSUER_ID", "ASC_KEY_PATH")
  if [key_id, issuer, key_path].any? { |v| v.to_s.empty? }
    warn "certificates: ASC_KEY_ID, ASC_ISSUER_ID and ASC_KEY_PATH are not all set; nothing asked"
    exit 0
  end
  begin
    bearer = token(File.read(key_path), key_id, issuer)
    answer = ask(Net::HTTP::Get, "/v1/certificates?limit=200", bearer)
    unless answer.is_a?(Net::HTTPSuccess)
      warn "certificates: Apple answered #{answer.code} to the list; nothing changed"
      exit 0
    end
    certificates = JSON.parse(answer.body).fetch("data", [])
    puts "certificates: #{certificates.length} on the account"
    certificates.each do |certificate|
      attributes = certificate.fetch("attributes", {})
      left = left_by_a_runner?(attributes)
      puts "  #{attributes["certificateType"]}  #{attributes["displayName"] || attributes["name"]}  until #{attributes["expirationDate"].to_s[0, 10]}#{left ? "  (a runner left it)" : ""}"
      next unless revoke && left

      gone = ask(Net::HTTP::Delete, "/v1/certificates/#{certificate["id"]}", bearer)
      puts gone.is_a?(Net::HTTPSuccess) ? "    revoked" : "    not revoked: Apple answered #{gone.code}"
    end
  rescue StandardError => e
    warn "certificates: #{e.class}: #{e.message}; nothing more done"
  end
end
