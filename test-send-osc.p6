use Net::OSC;
use Net::OSC::Types;

my Net::OSC::Server::UDP $server .= new(
  :listening-address<127.0.0.1>
  :listening-port(7658)
  :send-to-address<127.0.0.1> # ← Optional but makes sending to a single host very easy!
  :send-to-port(10009)        # ↲
  :actions(
    action(
      "/hello",
      sub ($msg, $match) {
        if $msg.type-string eq 's' {
          say "Hello { $msg.args[0] }!";
        }
        else {
          say "Hello?";
        }
      }
    ),
  )
);

# Send some messages!
$server.send: '/hello', :args('world', );
$server.send: '/hello', :args('lamp', );
$server.send: '/hello';

# Our routing will ignore this message:
$server.send: '/hello/not-really';

# Send a message to someone else?
try $server.send: '/hello', :args('out there', ), :address<192.168.1.1>, :port(54321);

given $server {
    .send: '/midi_sender/play', :args(osc-double(0.0), osc-int64(500), 1, 60, 60);
    .send: '/midi_sender/play', :args(osc-double(1.0), osc-int64(500), 1, 60, 60);
    .send: '/midi_sender/play', :args(osc-double(2.0), osc-int64(500), 1, 60, 60)
}

#Allow some time for our messages to arrive
sleep 0.5;

# Give our server a chance to say good bye if it needs too.
$server.close;