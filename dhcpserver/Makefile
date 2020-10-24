init:
	docker build -t dhcp-server .
	docker run --rm -it -v `pwd`/shared:/dhcp/shared dhcp-server
