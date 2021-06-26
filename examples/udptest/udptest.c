#include <unistd.h>
#include <arpa/inet.h>
#include <stdio.h> //printf
#include <stdlib.h> //exit(0);

#define N 10000

void die(char *s)
{
  perror(s);
  exit(1);
}

int main(void)
{
  unsigned short p[N];
  struct sockaddr_in me, peer;
  int s;
  socklen_t size_of_me=sizeof(me);

  for (unsigned short i = 0; i<N; i++)
  {
    if ((s = socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP)) == -1)
    {
      die("socket");
    }
    peer.sin_family = AF_INET;
    peer.sin_addr.s_addr=htonl(0x7f000001); // 127.0.0.1
    peer.sin_port=htons(30000+i);

    //send the message
    if (sendto(s, "foobar", 6, 0, (struct sockaddr *) &peer, sizeof(peer)) == -1)
    {
      die("sendto()");
    }
    if (getsockname(s, (struct sockaddr *) &me, &size_of_me))
    {
      die("getsockname");
    }
    if (sizeof(me)!=size_of_me)
    {
      printf("Oops, sizeof(me)=%lu, size_of_me=%u\n",sizeof(me),size_of_me);
      die("oops");
    }
    p[i]=ntohs(me.sin_port);

    close(s);
  }

  for (unsigned int j=0;j<N;j++)
  {
    printf("%hu\n",p[j]);
  }
  return 0;
}
